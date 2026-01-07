use crate::config::Config;
use crate::server::BackendWrapper;
use crate::workspace::ProjectState;
use crate::common::{point_to_byte, position_to_point, advance_point, get_class_name_from_root};
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
use crate::document;
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
        let current_text = params.text_document.text;
        let version = params.text_document.version;
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
        let project_url = self.0.find_parent_workspace(uri.clone()).unwrap();
        // TODO: what do I really want to do if the project doesn't exist? Panic? Create it? Wait?
        let project = self.0.get_project(&project_url).unwrap();
        let documents = project.documents.read();
        let curr_document = documents.get(&uri);
        if curr_document.is_none() {
            drop(documents);
            let new_tree = if file_type == FileType::Cls {
                project.parsers.cls.lock().parse(&current_text, None).unwrap()
            } else {
                project.parsers.routine.lock().parse(&current_text, None).unwrap()
            };
            let document = document::Document::new(current_text.clone(), new_tree.clone(), file_type.clone());
            if file_type == FileType::Cls {
                let class_name = get_class_name_from_root(&current_text, new_tree.root_node().clone());
                project.add_document(uri,document,class_name);
            }
            else {
                project.add_routine_document(uri,document);
            }
        }
        else {
            let curr_document_content = curr_document.unwrap().content.clone();
            let curr_document_file_type = curr_document.unwrap().file_type.clone();
            let curr_version = if curr_document.unwrap().version.is_none() {
                -1
            } else {
                curr_document.unwrap().version.unwrap()
            };
            drop(documents);
            if curr_document_content.as_str() != current_text.as_str() || curr_document_file_type != file_type {
                let new_tree = if file_type == FileType::Cls {
                    project.parsers.cls.lock().parse(&current_text, None).unwrap()
                } else {
                    project.parsers.routine.lock().parse(&current_text, None).unwrap()
                };
                let mut documents = project.documents.write();
                let document = documents.get_mut(&uri).unwrap();
                document.update(true, true, Some(new_tree), true, Some(file_type), true, Some(version));
            }
            else {
                if curr_version == -1 || version != curr_version {
                    let mut documents = project.documents.write();
                    let document = documents.get_mut(&uri).unwrap();
                    document.update(false, false, None, false, None, true, Some(version));
                    drop(documents);
                }
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = uri.path();
        if !path.ends_with(".cls") && !path.ends_with(".mac") && !path.ends_with(".inc") {
            return;
        }

        let project_url = self.0.find_parent_workspace(uri.clone()).unwrap();
        // TODO: what do I really want to do if the project doesn't exist? Panic? Create it? Wait?
        let project = self.0.get_project(&project_url).unwrap();

        let (file_type, mut current_text, curr_version, mut curr_tree) = {
            let documents = project.documents.read();
            let curr_document = documents.get(&uri).unwrap();
            let curr_version = curr_document.version.unwrap_or(0).clone();
            let current_text = curr_document.content.clone();
            let curr_tree = curr_document.tree.clone();

            (curr_document.file_type.clone(), current_text, curr_version, curr_tree)
        };
        let new_version = params.text_document.version;
        if new_version < curr_version{
            panic!("New version {:?} is less than old version {:?}", new_version, curr_version);
        }
        // if range and range_length are omitted, the text contains the full text, otherwise
        // it only contains the changes
        for change in params.content_changes {
            if let Some(range) = change.range {
                let new_text = change.text.as_str();
                let start_position = position_to_point(current_text.as_str(), range.start);
                let start_byte = point_to_byte(current_text.as_str(), start_position);

                let old_end_position = position_to_point(current_text.as_str(), range.end);
                let old_end_byte = point_to_byte(current_text.as_str(), old_end_position);

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
                current_text.replace_range(start_byte..old_end_byte, new_text);
                curr_tree.edit(&input_edit);
            }

            else {
                let old_text = current_text.as_str();
                let new_text = change.text;
                let input_edit = InputEdit {
                    start_byte: 0,
                    old_end_byte: old_text.len(),
                    new_end_byte: new_text.len(),
                    start_position: Point { row: 0, column: 0 },
                    old_end_position: advance_point(0, 0, old_text),
                    new_end_position: advance_point(0, 0, &new_text),
                };
                current_text = new_text;
                curr_tree.edit(&input_edit);
            }
        }

        let new_tree = if file_type == FileType::Cls {
            project.parsers.cls.lock().parse(&current_text, Some(&curr_tree)).unwrap()
        } else {
            project.parsers.routine.lock().parse(&current_text, Some(&curr_tree)).unwrap()
        };

        let mut documents = project.documents.write();
        let curr_document = documents.get_mut(&uri).unwrap();
        curr_document.content = current_text;
        curr_document.tree = new_tree;
        curr_document.version = Some(new_version);
    }

    // async fn did_close(&self, params: DidCloseTextDocumentParams) {}
}
