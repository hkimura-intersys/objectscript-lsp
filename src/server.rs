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
use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};
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
    fn get_project(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        self.projects.read().get(uri).cloned()
    }

    pub(crate) async fn index_workspace_scope(&self, uri: &Url) {
        let workspace = self.get_project(uri).expect("No Project Found");
        let root: PathBuf = workspace
            .root_path()
            .expect("workspace root not set")
            .to_path_buf();

        let project = Arc::clone(&workspace);

        // Run indexing on Tokio's blocking thread pool
        let handle = tokio::task::spawn_blocking(move || {
            // Parsers must be created inside this closure (they're not Send-safe to share across threads)
            let mut cls_parser = tree_sitter::Parser::new();
            cls_parser
                .set_language(&LANGUAGE_OBJECTSCRIPT.into())
                .expect("Error loading ObjectScript grammar");

            let mut routine_parser = tree_sitter::Parser::new();
            routine_parser
                .set_language(&LANGUAGE_OBJECTSCRIPT_CORE.into())
                .expect("Error loading Core ObjectScript grammar");

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
                    routine_parser.parse(&code, None)
                } else {
                    cls_parser.parse(&code, None)
                };

                let Some(tree) = tree_opt else { continue };
                let class_name = get_class_name_from_root(code.as_str(), tree.root_node());
                let doc = document::Document::new(code, tree, filetype, url.clone());
                // initial build: class keywords (procedure block, language), name,
                //                method names, method keywords (private, language, code mode, public list)
                project.add_document(url, doc, class_name);
            }
            // adds inheritance
            project.second_iteration();
        });

        // Wait for completion (and handle join errors)
        if let Err(join_err) = handle.await {
            eprintln!("index_workspace_scope spawn_blocking failed: {join_err:?}");
        }
    }
}
