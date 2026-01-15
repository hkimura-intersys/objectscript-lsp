use crate::common::{advance_point, get_string_at_byte_range, point_to_byte, position_to_point, ts_range_to_lsp_range};
use crate::config::Config;
use crate::parse_structures::{FileType};
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
    DidChangeWatchedFilesRegistrationOptions,
    DidOpenTextDocumentParams, FileSystemWatcher, GlobPattern,
    ImplementationProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, Location, MessageType, OneOf, Registration, ServerCapabilities, ServerInfo,
    TextDocumentClientCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    WatchKind
};
use tower_lsp::LanguageServer;
use tree_sitter::{InputEdit, Point, Tree};

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
    }

    async fn goto_implementation(&self, params: GotoImplementationParams) -> Result<Option<GotoImplementationResponse>> {
        self.0.client.log_message(MessageType::INFO, "goto implementation called").await;
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let mut locations = Vec::new();
        let Some(project) = self.0.get_project_from_document_url(&uri) else {
            self.0.client.log_message(MessageType::ERROR, "Failed to get project from document").await;
            return Ok(None);
        };
        let doc_snapshot: Option<(String, Tree)> = {
            let data = project.data.read();
            data.documents.get(&uri).map(|d| (d.content.clone(), d.tree.clone()))
        };

        let (content, tree) = match doc_snapshot {
            Some(v) => v,
            None => {
                self.0.client.log_message(MessageType::ERROR, "Failed to get document").await;
                return Ok(None);
            }
        };
        let content = content.as_str();
        // find what node is at that position
        // convert position to point, and find smallest node that has the range of that Point
        let point = position_to_point(content, position);
        let Some(node) = tree.root_node().named_descendant_for_point_range(point, point) else {
            self.0.client.log_message(MessageType::ERROR, "Failed to get node point descendant").await;
            return Ok(None);
        };

        if node.kind() == "identifier" {
            let Some(parent_node) = node.parent() else {
                self.0.client.log_message(MessageType::ERROR, "Failed to get parent node of identifier").await;
                return Ok(None);
            };

            if parent_node.kind() == "identifier" {
                let Some(second_parent_node) = parent_node.parent() else {
                    self.0.client.log_message(MessageType::ERROR, "Failed to get parent node of identifier").await;
                    return Ok(None);
                };

                if second_parent_node.kind() == "method_definition" {
                    self.0.client.log_message(MessageType::INFO, "IN METHOD DEF").await;
                    let Some(method_name) = get_string_at_byte_range(content, node.byte_range()) else {
                        self.0.client.log_message(MessageType::ERROR, "Failed to get method name").await;
                        return Ok(None);
                    };
                    self.0.client.log_message(MessageType::INFO, "GOT METHOD NAME").await;
                    // node is a method name
                    let overrides = {
                        let data = project.data.read();
                        data.get_method_overrides(uri, method_name)
                    };
                    self.0.client.log_message(MessageType::INFO, format!("OVERRIDES {:?}", overrides).as_str()).await;
                    for (uri, range) in &overrides {
                        let data = project.data.read();
                        let Some(document_content) = data.documents.get(uri).map(|d| d.content.as_str()) else {
                            eprintln!("Failed to get document of uri {}", uri);
                            continue;
                        };
                        let lsp_range = ts_range_to_lsp_range(document_content, *range);
                        let location = Location {
                            uri: uri.clone(),
                            range: lsp_range
                        };
                        locations.push(location);
                    }
                }

                else {
                    self.0.client.log_message(MessageType::INFO, format!("Parent Node is not a method definition, it is a {:?}", parent_node.kind())).await;
                }
            }

            else {
                self.0.client.log_message(MessageType::INFO, format!("Parent Node is not an identifier, it is a {:?}", parent_node.kind())).await;
            }
        }

        self.0.client
            .log_message(MessageType::INFO, format!("locations: {:?}", locations))
            .await;

        if locations.len() == 1 {
            Ok(Some(GotoImplementationResponse::Scalar(locations[0].clone())))
        }
        else if locations.is_empty() {
            self.0.client.log_message(MessageType::ERROR, "Failed to get document location").await;
            Ok(None)
        }
        else {
            Ok(Some(GotoImplementationResponse::Array(locations)))
        }
    }

    async fn shutdown(&self) -> Result<()> {
        // need to look more into if this is good for doing nothing
        exit(0)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
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
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
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
        // if range and range_length are omitted, the text contains the full text, otherwise
        // it only contains the changes
        for change in params.content_changes {
            if let Some(range) = change.range {
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
                // update content string with new string for the changed range
                old_text.replace_range(start_byte..old_end_byte, new_text);
                old_tree.edit(&input_edit);
            } else {
                let old_text_str = old_text.as_str();
                let new_text = change.text;
                let input_edit = InputEdit {
                    start_byte: 0,
                    old_end_byte: old_text.len(),
                    new_end_byte: new_text.len(),
                    start_position: Point { row: 0, column: 0 },
                    old_end_position: advance_point(0, 0, old_text_str),
                    new_end_position: advance_point(0, 0, &new_text),
                };
                old_text = new_text;
                old_tree.edit(&input_edit);
            }
        }

        let parsed: Option<tree_sitter::Tree> = {
            if file_type == FileType::Cls {
                let mut parser = project.parsers.cls.lock();
                parser.parse(&old_text, Some(&old_tree))
            } else {
                let mut parser = project.parsers.routine.lock();
                parser.parse(&old_text, Some(&old_tree))
            }
        }; // <-- lock guard DROPS here

        let new_tree = match parsed {
            Some(t) => t,
            None => {
                self.0
                    .client
                    .log_message(MessageType::WARNING, "Incremental parse failed".to_string())
                    .await;
                return;
            }
        };

        project.update_document(uri, new_tree, file_type, new_version, &old_text);
    }

    // async fn did_close(&self, params: DidCloseTextDocumentParams) {}
}
