use crate::config::Config;
use crate::server::BackendWrapper;
use crate::workspace::ProjectState;
use crate::common::{point_to_byte, position_to_point, advance_point, ts_range_to_lsp_range, point_in_range};
use parking_lot::RwLock;
use serde_json;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tree_sitter::{Point, InputEdit};
use tower_lsp::lsp_types::{DidChangeTextDocumentParams, DidChangeWatchedFilesRegistrationOptions, DidCloseTextDocumentParams, DidOpenTextDocumentParams, FileSystemWatcher, GlobPattern, InitializeParams, InitializeResult, InitializedParams, Registration, ServerCapabilities, ServerInfo, TextDocumentClientCapabilities, WatchKind, CodeActionProviderCapability, OneOf, TextDocumentSyncCapability, TextDocumentSyncKind, ImplementationProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, Location, Url};
use tower_lsp::LanguageServer;
use crate::parse_structures::{FileType, PublicMethodRef};

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

        // Optional but nice:
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),

        // For your “dependencies” demo (section 3)
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
                let project_root = folder.uri.to_file_path().unwrap();
                // create projectState and set the projectRoot
                let state = ProjectState::new();
                state
                    .project_root_path
                    .set(Some(project_root))
                    .expect("project root should only ever be set in initialize");

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

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let tdp = params.text_document_position_params;
        let uri = tdp.text_document.uri;
        let pos = tdp.position;

        let project = self.0.get_project_from_document_url(&uri);

        // --- Snapshot current doc info (avoid holding locks long) ---
        let (content, tree_point, scope_tree, class_id_opt, class_name) = {
            let docs = project.documents.read();
            let doc = match docs.get(&uri) {
                Some(d) => d,
                None => return Ok(None),
            };
            (
                doc.content.clone(),
                doc.tree.clone(),
                doc.scope_tree.clone(),
                doc.class_id,
                doc.class_name.clone(),
            )
        };

        let Some(class_id) = class_id_opt else {
            return Ok(None);
        };

        let point = position_to_point(&content, pos);

        // Helper: build an LSP Location from a tree-sitter Range + target uri
        let mk_location = |target_uri: Url, ts_range: tree_sitter::Range| -> Location {
            // Prefer correct UTF-16 conversion using the *target* document's content if we have it.
            let target_text_opt = project
                .documents
                .read()
                .get(&target_uri)
                .map(|d| d.content.clone());

            let lsp_range = if let Some(t) = target_text_opt {
                ts_range_to_lsp_range(&t, ts_range)
            } else {
                // fallback: treat columns as bytes (usually OK for ASCII, but best-effort)
                tower_lsp::lsp_types::Range {
                    start: tower_lsp::lsp_types::Position {
                        line: ts_range.start_point.row as u32,
                        character: ts_range.start_point.column as u32,
                    },
                    end: tower_lsp::lsp_types::Position {
                        line: ts_range.end_point.row as u32,
                        character: ts_range.end_point.column as u32,
                    },
                }
            };

            Location {
                uri: target_uri,
                range: lsp_range,
            }
        };

        // Helper: resolve a PublicMethodRef -> (Url, Range)
        let resolve_public_ref = |mref: PublicMethodRef| -> Option<(Url, tree_sitter::Range)> {
            let gsm = project.global_semantic_model.read();

            let cls_name = gsm.classes.get(mref.class.0)?.name.clone();
            let class_symbol_id = *project.class_defs.read().get(&cls_name)?;

            let methods = gsm.methods.get(&mref.class)?;
            let method_name = methods.get(mref.id.0)?.name.clone();

            let method_sym_id = project
                .pub_method_defs
                .read()
                .get(&cls_name)?
                .get(&method_name)?
                .clone();

            let sym = gsm
                .method_defs
                .get(&class_symbol_id)?
                .get(method_sym_id.0)?
                .clone();

            Some((sym.url, sym.location))
        };

        // Helper: resolve a private method in *this* doc by name
        let resolve_private_in_this_doc =
            |name: &str| -> Option<(Url, tree_sitter::Range)> {
                let (scope_id, sym_id) = scope_tree.get_private_method_symbol(name)?;
                let scopes = scope_tree.scopes.read();
                let scope = scopes.get(&scope_id)?;
                let sym = scope.method_symbols.get(sym_id.0)?.clone();
                Some((uri.clone(), sym.location))
            };

        // Helper: resolve a public method in *this class* by name (even if callsite wasn’t resolved)
        let resolve_public_in_this_class =
            |method_name: &str| -> Option<(Url, tree_sitter::Range)> {
                let gsm = project.global_semantic_model.read();
                let class_symbol_id = *project.class_defs.read().get(&class_name)?;

                let method_sym_id = project
                    .pub_method_defs
                    .read()
                    .get(&class_name)?
                    .get(method_name)?
                    .clone();

                let sym = gsm
                    .method_defs
                    .get(&class_symbol_id)?
                    .get(method_sym_id.0)?
                    .clone();

                Some((sym.url, sym.location))
            };

        // --- 1) Prefer: if cursor is within a recorded callsite, use that ---
        let callsite_opt = {
            let gsm = project.global_semantic_model.read();
            let Some(cls) = gsm.classes.get(class_id.0) else {
                return Ok(None);
            };
            cls.method_calls
                .iter()
                .find(|s| point_in_range(point, s.call_range.start_point, s.call_range.end_point))
                .cloned()
        };

        if let Some(site) = callsite_opt {
            // If already resolved via override index, this is the most correct.
            if let Some(mref) = site.callee_symbol {
                if let Some((target_uri, ts_range)) = resolve_public_ref(mref) {
                    let loc = mk_location(target_uri, ts_range);
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }
            }

            // Otherwise: try private in this file (relative-dot calls etc.)
            if let Some((target_uri, ts_range)) = resolve_private_in_this_doc(&site.callee_method) {
                let loc = mk_location(target_uri, ts_range);
                return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
            }

            // Fallback: maybe it’s a public method in this class, but unresolved/stale index
            if let Some((target_uri, ts_range)) = resolve_public_in_this_class(&site.callee_method) {
                let loc = mk_location(target_uri, ts_range);
                return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
            }

            return Ok(None);
        }

        // --- 2) Fallback: token under cursor -> try private then public in this class ---
        let node = tree_point
            .root_node()
            .named_descendant_for_point_range(point, point)
            .unwrap_or_else(|| tree_point.root_node());

        let token = content
            .get(node.byte_range())
            .unwrap_or("")
            .trim()
            .to_string();

        if token.is_empty() {
            return Ok(None);
        }

        if let Some((target_uri, ts_range)) = resolve_private_in_this_doc(&token) {
            let loc = mk_location(target_uri, ts_range);
            return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
        }

        if let Some((target_uri, ts_range)) = resolve_public_in_this_class(&token) {
            let loc = mk_location(target_uri, ts_range);
            return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
        }

        Ok(None)
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

        let registration = Registration {
            id: "ObjectScriptCacheWatcher".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(serde_json::to_value(options).unwrap()),
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
            self.0.handle_did_open(uri, params.text_document.text, file_type, params.text_document.version).await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = uri.path();
        if !path.ends_with(".cls") && !path.ends_with(".mac") && !path.ends_with(".inc") {
            return;
        }
        let project = self.0.get_project_from_document_url(&uri);
        let (file_type, mut old_text, old_version, mut old_tree) = project.get_document_info(&uri);

        let new_version = params.text_document.version;
        if new_version < old_version {
            panic!("New version {:?} is less than old version {:?}", new_version, old_version);
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
                let new_end_position = advance_point(start_position.row, start_position.column, new_text);
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
            }

            else {
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

        let new_tree = if file_type == FileType::Cls {
            project.parsers.cls.lock().parse(&old_text, Some(&old_tree)).unwrap()
        } else {
            project.parsers.routine.lock().parse(&old_text, Some(&old_tree)).unwrap()
        };
        project.update_document(uri,new_tree,file_type, new_version, &old_text);
    }

    // async fn did_close(&self, params: DidCloseTextDocumentParams) {}
}
