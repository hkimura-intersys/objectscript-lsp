use crate::config::Config;
use crate::server::BackendWrapper;
use crate::workspace::ProjectState;
use parking_lot::RwLock;
use serde_json;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesRegistrationOptions,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, FileSystemWatcher, GlobPattern,
    InitializeParams, InitializeResult, InitializedParams, Registration, ServerCapabilities,
    ServerInfo, TextDocumentClientCapabilities, WatchKind,
};
use tower_lsp::LanguageServer;
// Lazy Lock wrapper - provides lazy, one time initialization (so the hashmap isn't created when the program starts, it is created on first access, when the first doc is opened).
// the initialization happens exactly once, even if multiple threads try to access it simultaneously.
// pub static PROJECT_DOCUMENTS: LazyLock<Arc<RwLock<HashMap<Url,Document>>>> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

// deciding between RwLock and DashMap
// Atomic Bool: https://doc.rust-lang.org/std/sync/atomic/struct.AtomicBool.html
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
        let enable_format = negotiations.is_formatting_enabled();
        let enable_lint = negotiations.is_lint_enabled();
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
                self.0.add_project(folder.uri, state).await;
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
                    let _ = backend.index_workspace_scope(&workspace.uri).await;
                });
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        // need to look more into if this is good for doing nothing
        exit(0)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {}

    async fn did_change(&self, params: DidChangeTextDocumentParams) {}

    async fn did_close(&self, params: DidCloseTextDocumentParams) {}
}
