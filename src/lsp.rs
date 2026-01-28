use crate::common::{
    advance_point, generic_exit_statements, generic_skipping_statements, get_class_name_from_root,
    get_string_at_byte_range, method_name_from_identifier_node, point_to_byte, position_to_point,
    start_of_function, successful_exit, ts_range_to_lsp_range,
};
use crate::config::Config;
use crate::parse_structures::FileType;
use crate::server::BackendWrapper;
use crate::workspace::ProjectState;
use parking_lot::RwLock;
use serde_json;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::request::{GotoImplementationParams, GotoImplementationResponse};
use tower_lsp::lsp_types::{
    CodeActionProviderCapability, DidChangeTextDocumentParams,
    DidChangeWatchedFilesRegistrationOptions, DidOpenTextDocumentParams, FileSystemWatcher,
    GlobPattern, GotoDefinitionParams, GotoDefinitionResponse, ImplementationProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, Location, MessageType, OneOf,
    Registration, ServerCapabilities, ServerInfo, TextDocumentClientCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, WatchKind,
};
use tower_lsp::LanguageServer;
use tree_sitter::{InputEdit, Tree};

static ENABLE_SNIPPETS: AtomicBool = AtomicBool::new(false);
static CLIENT_CAPABILITIES: RwLock<Option<TextDocumentClientCapabilities>> = RwLock::new(None);

fn set_client_text_document(text_document: Option<TextDocumentClientCapabilities>) {
    let mut data = CLIENT_CAPABILITIES.write();
    *data = text_document;
}

pub fn get_client_capabilities() -> Option<TextDocumentClientCapabilities> {
    let data = CLIENT_CAPABILITIES.read();
    data.clone()
}

fn build_caps(cfg: &Config) -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        document_formatting_provider: cfg.enable_formatting.then_some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

        // TODO: need to do dotted statement formatting
        // document_formatting_provider: cfg.enable_formatting.then_some(OneOf::Left(true)),
        ..Default::default()
    }
}

pub fn are_snippets_enabled() -> bool {
    if !ENABLE_SNIPPETS.load(Ordering::Relaxed) {
        return false;
    }
    match get_client_capabilities() {
        Some(c) => c
            .completion
            .and_then(|item| item.completion_item)
            .and_then(|item| item.snippet_support)
            .unwrap_or(false),
        _ => false,
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for BackendWrapper {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        start_of_function("LSP", "initialize");
        // negotiate w/ client to set config for formatting, lint, snippets
        let negotiations: Config = params
            .initialization_options
            .and_then(|v| serde_json::from_value::<Config>(v).ok())
            .unwrap_or_default();

        // set negotiated config
        ENABLE_SNIPPETS.store(negotiations.enable_snippets, Ordering::Relaxed);
        set_client_text_document(params.capabilities.text_document);

        if let Some(folders) = params.workspace_folders {
            for folder in folders {
                let Ok(project_root) = folder.uri.to_file_path() else {
                    self.0
                        .client
                        .log_message(MessageType::ERROR, "Failed to get project root path")
                        .await;
                    continue;
                };
                // create projectState and set the projectRoot
                let state = ProjectState::new();
                if state.project_root_path.set(Some(project_root)).is_err() {
                    self.0
                        .client
                        .log_message(
                            MessageType::WARNING,
                            "project_root_path was already set; ignoring duplicate initialize",
                        )
                        .await;
                }

                // add projectState to projects
                self.0.add_project(folder.uri, state);
            }
        }
        successful_exit("LSP", "initialize");
        Ok(InitializeResult {
            capabilities: build_caps(&negotiations),
            server_info: Some(ServerInfo {
                name: "objectscript-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        // register watchers for any .cls, .mac, and .inc files in the workspace
        start_of_function("LSP", "initialized");
        let globs = ["**/*.cls", "**/*.mac", "**/*.inc"];
        let watchers = globs
            .into_iter()
            .map(|g| FileSystemWatcher {
                glob_pattern: GlobPattern::String(g.to_string()).into(),
                kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            })
            .collect();
        let options = DidChangeWatchedFilesRegistrationOptions { watchers };

        let register_options = match serde_json::to_value(options) {
            Ok(v) => Some(v),
            Err(e) => {
                self.0
                    .client
                    .log_message(MessageType::ERROR, &e.to_string())
                    .await;
                None
            }
        };

        let registration = Registration {
            id: "ObjectScriptCacheWatcher".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options,
        };

        self.0
            .client
            .register_capability(vec![registration])
            .await
            .ok();

        if let Ok(Some(folders)) = self.0.client.workspace_folders().await {
            for workspace in folders {
                let backend = Arc::clone(&self.0);
                tokio::spawn(async move {
                    let _ = backend.index_workspace(&workspace.uri).await;
                });
            }
        }
        successful_exit("LSP", "initialized");
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        start_of_function("LSP", "goto_definition");
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let mut locations: Vec<Location> = Vec::new();
        let Some(project) = self.0.get_project_from_document_url(&uri) else {
            self.0
                .client
                .log_message(MessageType::ERROR, "Failed to get project from document")
                .await;
            generic_exit_statements("LSP", "goto_definition");
            return Ok(None);
        };
        let doc_snapshot: Option<(String, Tree)> = {
            let data = project.data.read();
            data.documents
                .get(&uri)
                .map(|d| (d.content.clone(), d.tree.clone()))
        };

        let (content, tree) = match doc_snapshot {
            Some(v) => v,
            None => {
                self.0
                    .client
                    .log_message(MessageType::ERROR, "Failed to get document")
                    .await;
                return Ok(None);
            }
        };
        let content = content.as_str();
        // find what node is at that position
        // convert position to point, and find smallest node that has the range of that Point
        let point = position_to_point(content, position);

        let Some(node) = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
        else {
            eprintln!(
                "Error: failed to get node that encapsulates point: {:?}",
                point
            );
            generic_exit_statements("LSP", "goto_definition");
            return Ok(None);
        };

        let Some(symbol_string) = content.get(node.byte_range()) else {
            eprintln!(
                "Error: failed to get string content of the node: {:?}",
                node
            );
            generic_exit_statements("LSP", "goto_definition");
            return Ok(None);
        };

        if node.kind() == "objectscript_identifier" {
            // get method name
            let Some(method_name) = method_name_from_identifier_node(node, content, 0) else {
                generic_exit_statements("LSP", "goto_definition");
                return Ok(None);
            };

            self.0
                .client
                .log_message(
                    MessageType::INFO,
                    format!("Getting definitions for symbol: {}", symbol_string),
                )
                .await;
            let data = project.data.read();

            // get location of symbol
            for (url, range) in data.get_variable_symbol_location(
                uri,
                point,
                symbol_string.to_string(),
                method_name,
            ) {
                let Some(document) = data.documents.get(&url) else {
                    eprintln!("Error: Couldn't get document content");
                    generic_skipping_statements("goto_definition", url.path(), "Symbol location");
                    continue;
                };
                let document_content = document.content.as_str();
                let lsp_range = ts_range_to_lsp_range(document_content, range);
                let location = Location {
                    uri: url.clone(),
                    range: lsp_range,
                };
                locations.push(location);
            }

            return if locations.is_empty() {
                eprintln!("Error: Symbol is not defined in this workspace.");
                successful_exit("LSP", "goto_definition");
                Ok(None)
            } else if locations.len() == 1 {
                successful_exit("LSP", "goto_definition");
                Ok(Some(GotoDefinitionResponse::Scalar(locations[0].clone())))
            } else {
                successful_exit("LSP", "goto_definition");
                Ok(Some(GotoDefinitionResponse::Array(locations)))
            };
        }

        self.0
            .client
            .log_message(
                MessageType::ERROR,
                format!("goto_definition not yet implemented for: {:?}", node.kind()),
            )
            .await;
        successful_exit("LSP", "goto_definition");
        Ok(None)
    }

    async fn goto_implementation(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        start_of_function("LSP", "goto_implementation");
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let mut locations = Vec::new();
        let Some(project) = self.0.get_project_from_document_url(&uri) else {
            self.0
                .client
                .log_message(MessageType::ERROR, "Failed to get project from document")
                .await;
            generic_exit_statements("LSP", "goto_implementation");
            return Ok(None);
        };
        let doc_snapshot: Option<(String, Tree)> = {
            let data = project.data.read();
            data.documents
                .get(&uri)
                .map(|d| (d.content.clone(), d.tree.clone()))
        };

        let (content, tree) = match doc_snapshot {
            Some(v) => v,
            None => {
                self.0
                    .client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to get document for url: {:?}", uri.path()),
                    )
                    .await;

                return Ok(None);
            }
        };
        let content = content.as_str();
        // find what node is at that position
        // convert position to point, and find smallest node that has the range of that Point
        let point = position_to_point(content, position);
        let Some(class_name) = get_class_name_from_root(content, tree.root_node()) else {
            return Ok(None);
        };
        let Some(node) = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
        else {
            self.0
                .client
                .log_message(
                    MessageType::ERROR,
                    format!("Failed to get node at point: {:?}", point),
                )
                .await;
            generic_exit_statements("LSP", "goto_implementation");
            return Ok(None);
        };

        if node.kind() == "identifier" {
            let Some(parent_node) = node.parent() else {
                self.0
                    .client
                    .log_message(
                        MessageType::ERROR,
                        "Warning: Expected identifier node to have a parent, but it did not.",
                    )
                    .await;
                generic_exit_statements("LSP", "goto_implementation");
                return Ok(None);
            };

            if parent_node.kind() == "identifier" {
                let Some(second_parent_node) = parent_node.parent() else {
                    self.0
                        .client
                        .log_message(
                            MessageType::ERROR,
                            "Warning: Expected identifier node to have a parent, but it did not.",
                        )
                        .await;
                    generic_exit_statements("LSP", "goto_implementation");
                    return Ok(None);
                };

                if second_parent_node.kind() == "method_definition" {
                    let Some(method_name) = get_string_at_byte_range(content, node.byte_range())
                    else {
                        self.0
                            .client
                            .log_message(
                                MessageType::ERROR,
                                format!("Error: failed to get string for node {:?}", node),
                            )
                            .await;
                        generic_exit_statements("LSP", "goto_implementation");
                        return Ok(None);
                    };
                    // node is a method name
                    let overrides = {
                        let data = project.data.read();
                        data.get_method_overrides(uri, method_name.clone())
                    };
                    self.0.client.log_message(
                        MessageType::INFO,
                        format!("According to the Override Index, subclass method implementations of the method named {:?} from Class named {:?} are located here: {:?}", method_name.clone(), class_name, overrides),
                    ).await;
                    for (uri, range) in &overrides {
                        let data = project.data.read();
                        let Some(document_content) =
                            data.documents.get(uri).map(|d| d.content.as_str())
                        else {
                            eprintln!("Error: failed to get document of uri {}", uri.path());
                            generic_skipping_statements(
                                "goto_implementation",
                                uri.path(),
                                "Url, failed to get document",
                            );
                            continue;
                        };
                        let lsp_range = ts_range_to_lsp_range(document_content, *range);
                        let location = Location {
                            uri: uri.clone(),
                            range: lsp_range,
                        };
                        locations.push(location);
                    }
                } else {
                    self.0
                        .client
                        .log_message(
                            MessageType::INFO,
                            "Symbol is not a method name, goto_implementation is for methods only.",
                        )
                        .await;
                    generic_exit_statements("LSP", "goto_implementation");
                    return Ok(None);
                }
            } else {
                self.0
                    .client
                    .log_message(
                        MessageType::INFO,
                        "Symbol is not a method name, goto_implementation is for methods only.",
                    )
                    .await;
                generic_exit_statements("LSP", "goto_implementation");
                return Ok(None);
            }
        } else {
            self.0
                .client
                .log_message(
                    MessageType::INFO,
                    "Symbol is not a method name, goto_implementation is for methods only.",
                )
                .await;
            generic_exit_statements("LSP", "goto_implementation");
            return Ok(None);
        }
        if locations.len() == 1 {
            successful_exit("LSP", "goto_implementation");
            Ok(Some(GotoImplementationResponse::Scalar(
                locations[0].clone(),
            )))
        } else if locations.is_empty() {
            self.0
                .client
                .log_message(
                    MessageType::WARNING,
                    "No method implementations were found for the given symbol.",
                )
                .await;
            successful_exit("LSP", "goto_implementation");
            Ok(None)
        } else {
            successful_exit("LSP", "goto_implementation");
            Ok(Some(GotoImplementationResponse::Array(locations)))
        }
    }

    async fn shutdown(&self) -> Result<()> {
        // need to look more into if this is good for doing nothing
        exit(0)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        start_of_function("LSP", "did_open");
        let uri = params.text_document.uri;
        let path = uri.path();
        if !path.ends_with(".cls") && !path.ends_with(".mac") && !path.ends_with(".inc") {
            return;
        }
        let file_type = if path.ends_with(".cls") {
            FileType::Cls
        } else if path.ends_with(".mac") {
            FileType::Mac
        } else {
            FileType::Inc
        };

        if file_type == FileType::Cls {
            self.0.handle_did_open(
                uri,
                params.text_document.text,
                file_type,
                params.text_document.version,
            );
        }
        successful_exit("LSP", "did_open");
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.0
            .client
            .log_message(MessageType::INFO, "Did Change called")
            .await;
        let uri = params.text_document.uri;
        let path = uri.path();
        if !path.ends_with(".cls") {
            return;
        }
        let Some(project) = self.0.get_project_from_document_url(&uri) else {
            return;
        };
        let Some((file_type, mut old_text, old_version, mut old_tree)) =
            project.get_document_info(&uri)
        else {
            let new_version = params.text_document.version;
            let file_type = FileType::Cls;
            // Try to get current cached doc

            // Base text: prefer disk if available, otherwise empty.
            let mut text = if let Ok(p) = uri.to_file_path() {
                std::fs::read_to_string(p).unwrap_or_default()
            } else {
                String::new()
            };

            // Apply ranged changes to the base text (Zed may send initial full contents as range edit).
            for change in &params.content_changes {
                let Some(range) = change.range else {
                    // (Zed likely won't do this, but handle it anyway)
                    text = change.text.clone();
                    continue;
                };

                let start_point = position_to_point(&text, range.start);
                let start_byte = point_to_byte(&text, start_point);

                let end_point = position_to_point(&text, range.end);
                let end_byte = point_to_byte(&text, end_point);

                text.replace_range(start_byte..end_byte, &change.text);
            }

            let parsed: Option<Tree> = {
                let mut parser = project.parsers.cls.lock();
                parser.parse(&text, None)
            }; // lock guard drops here

            let new_tree = match parsed {
                Some(t) => t,
                None => {
                    self.0
                        .client
                        .log_message(
                            MessageType::WARNING,
                            "Incremental parse failed.".to_string(),
                        )
                        .await;
                    return;
                }
            };

            // Insert/update doc record so future incremental changes work
            {
                let Some(class_name) =
                    get_class_name_from_root(text.as_str(), new_tree.root_node())
                else {
                    eprintln!("Error: Failed to get class name");
                    return;
                };
                let mut data = project.data.write();
                data.add_document_if_absent(
                    uri.clone(),
                    text.clone(),
                    new_tree.clone(),
                    file_type,
                    class_name,
                    Some(new_version),
                );
            }
            return;
        };

        let new_version = params.text_document.version;
        if new_version < old_version {
            self.0
                .client
                .log_message(
                    MessageType::ERROR,
                    "New version {new_version} is less than old version {old_version}",
                )
                .await;
        }

        let full_snapshot = params
            .content_changes
            .iter()
            .rev()
            .find(|c| c.range.is_none())
            .map(|c| c.text.clone());

        let did_full_replace = full_snapshot.is_some();
        self.0
            .client
            .log_message(
                MessageType::INFO,
                format!("Full Replace: {:?}", did_full_replace),
            )
            .await;
        if let Some(new_full_text) = full_snapshot {
            // Full replace: overwrite text, DO NOT edit the old tree incrementally.
            old_text = new_full_text;
        } else {
            // Incremental edits: apply each ranged edit sequentially.
            for change in &params.content_changes {
                let range = change
                    .range
                    .expect("no full snapshot, so all changes must have ranges");
                let new_text = change.text.as_str();

                let start_position = position_to_point(old_text.as_str(), range.start);
                let start_byte = point_to_byte(old_text.as_str(), start_position);

                let old_end_position = position_to_point(old_text.as_str(), range.end);
                let old_end_byte = point_to_byte(old_text.as_str(), old_end_position);

                let new_end_byte = start_byte + new_text.len();
                let new_end_position =
                    advance_point(start_position.row, start_position.column, new_text);

                let input_edit = InputEdit {
                    start_byte,
                    old_end_byte,
                    new_end_byte,
                    start_position,
                    old_end_position,
                    new_end_position,
                };
                old_text.replace_range(start_byte..old_end_byte, new_text);
                old_tree.edit(&input_edit);
            }
        }

        let parsed: Option<Tree> = {
            if file_type == FileType::Cls {
                let mut parser = project.parsers.cls.lock();
                if did_full_replace {
                    parser.parse(&old_text, None)
                } else {
                    parser.parse(&old_text, Some(&old_tree))
                }
            } else {
                let mut parser = project.parsers.routine.lock();
                if did_full_replace {
                    parser.parse(&old_text, None)
                } else {
                    parser.parse(&old_text, Some(&old_tree))
                }
            }
        }; // lock guard drops here

        let new_tree = match parsed {
            Some(t) => t,
            None => {
                self.0
                    .client
                    .log_message(
                        MessageType::WARNING,
                        "Incremental parse failed.".to_string(),
                    )
                    .await;
                return;
            }
        };

        {
            let mut data = project.data.write();
            if let Some(doc) = data.documents.get_mut(&uri) {
                doc.content = old_text.clone();
                doc.tree = new_tree.clone();
                doc.version = Some(new_version);
                doc.file_type = file_type.clone();
            }
        }

        if new_tree.root_node().has_error() {
            self.0
                .client
                .log_message(
                    MessageType::ERROR,
                    format!("New Tree has Errors: {:?}", new_tree.root_node().to_sexp()),
                )
                .await;
        } else {
            project.update_document(uri, new_tree, file_type, new_version, old_text.as_str());
        }
    }

    // async fn did_close(&self, params: DidCloseTextDocumentParams) {}
}
