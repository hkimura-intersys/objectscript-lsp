use crate::common::get_class_name_from_root;
use crate::document;
use crate::parse_structures::FileType;
use crate::workspace::ProjectState;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;
use tower_lsp::Client;
use walkdir::WalkDir;

pub struct BackendWrapper(pub(crate) Arc<Backend>);
impl BackendWrapper {
    pub fn new(client: Client) -> Self {
        Self(Arc::new(Backend::new(client)))
    }
}
pub(crate) struct Backend {
    pub(crate) client: Client, // stored in the backend, and used to send messages/diagnostics to the editor
    pub(crate) projects: Arc<RwLock<HashMap<Url, Arc<ProjectState>>>>,
}

impl Backend {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            projects: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add Workspace and it's given State
    pub(crate) fn add_project(&self, uri: Url, state: ProjectState) {
        self.projects.write().insert(uri, Arc::new(state));
    }

    /// Get Project
    pub fn get_project(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        self.projects.read().get(uri).cloned()
    }

    /// Given a text document's Url, find the associated workspace
    pub fn find_parent_workspace(&self, uri: Url) -> Option<Url> {
        let doc_path: PathBuf = uri.to_file_path().ok()?;

        // find longest prefix
        let projects = self.projects.read();

        projects
            .keys()
            .filter_map(|ws_uri| {
                let ws_path = ws_uri.to_file_path().ok()?;
                if doc_path.starts_with(&ws_path) {
                    Some((ws_path.components().count(), ws_uri.clone()))
                } else {
                    None
                }
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, ws_uri)| ws_uri)
    }

    pub(crate) async fn index_workspace(&self, uri: &Url) {
        let workspace = self.get_project(uri).expect("No Project Found");
        let root: PathBuf = workspace
            .root_path()
            .expect("workspace root not set")
            .to_path_buf();

        let project = Arc::clone(&workspace);

        // Run indexing on Tokio's blocking thread pool
        let handle = tokio::task::spawn_blocking(move || {
            for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();

                let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                    continue;
                };

                let (filetype, use_core) = match ext {
                    "cls" => (FileType::Cls, false),
                    "inc" => (FileType::Inc, true),
                    "mac" => (FileType::Mac, true),
                    _ => continue,
                };

                let code = match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let url = match Url::from_file_path(path) {
                    Ok(u) => u,
                    Err(_) => continue,
                };

                let tree_opt = if use_core {
                    project.parsers.routine.lock().parse(&code, None)
                } else {
                    project.parsers.cls.lock().parse(&code, None)
                };

                let Some(tree) = tree_opt else { continue };
                let class_name = get_class_name_from_root(code.as_str(), tree.root_node());
                let doc = document::Document::new(code, tree, filetype);
                // initial build: class keywords (procedure block, language), name,
                //                method names, method keywords (private, language, code mode, public list)
                project.add_document(url, doc, class_name);
            }
            // adds inheritance
            project.build_inheritance_and_variables();
        });
        // Wait for completion (and handle join errors)
        if let Err(join_err) = handle.await {
            eprintln!("index_workspace_scope spawn_blocking failed: {join_err:?}");
        }
    }
}
