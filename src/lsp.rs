use crate::config::Config;
use crate::server::BackendWrapper;
use crate::workspace::ProjectState;
use crate::common::{point_to_byte, position_to_point, advance_point};
use parking_lot::RwLock;
use serde_json;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tree_sitter::{Point, InputEdit};
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesRegistrationOptions,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, FileSystemWatcher, GlobPattern,
    InitializeParams, InitializeResult, InitializedParams, Registration, ServerCapabilities,
    ServerInfo, TextDocumentClientCapabilities, WatchKind,
};
use tower_lsp::LanguageServer;
use crate::parse_structures::FileType;

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
            .and_then(|value| serde_json::from_value(value).unwrap_or(None))
            .unwrap_or_default();

        // set negotiated config
        ENABLE_SNIPPETS.store(negotiations.are_snippets_enabled(), Ordering::Relaxed);
        // let enable_format = negotiations.is_formatting_enabled();
        // let enable_lint = negotiations.is_lint_enabled();
        // TODO: where do I set this
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

        set_client_text_document(params.capabilities.text_document);
        let version: String = env!("CARGO_PKG_VERSION").to_string();
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: String::from("objectscript-lsp"),
                version: Some(version),
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
