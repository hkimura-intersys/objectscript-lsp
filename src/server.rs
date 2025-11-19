use crate::document;
use crate::scope_tree::ScopeTree;
use crate::workspace::ProjectState;
use parking_lot::RwLock;
use ropey::Rope;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
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
    pub(crate) async fn add_project(&self, uri: Url, state: ProjectState) {
        self.projects.write().insert(uri, Arc::new(state));
    }

    /// Get Project
    async fn get_project(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        let map = self.projects.read();
        map.get(uri).cloned()
    }

    async fn read_text_async(&self, path: &Path) -> anyhow::Result<String> {
        Ok(fs::read_to_string(path).await?)
    }

    pub(crate) async fn index_workspace_scope(&self, uri: &Url) {
        let project_state = self.get_project(uri).await.expect("No Project Found");
        // get the project root path
        let root = project_state.root_path().expect("workspace root not set");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&LANGUAGE_OBJECTSCRIPT.into())
            .expect("Error loading Objectscript grammar");
        let mut core_parser = tree_sitter::Parser::new();
        core_parser
            .set_language(&LANGUAGE_OBJECTSCRIPT_CORE.into())
            .expect("Error loading Core ObjectScript grammar");

        // TODO should I create another file and then a new thread from here so each file is indexed async?
        for file in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let path = file.path();
            let ext = path.extension().unwrap().to_str().unwrap();
            if ext == "cls" {
                let code = self.read_text_async(path).await.unwrap();
                // this is the first time parsing, so no other tree to pass in
                let tree = parser.parse(code.clone(), None).unwrap();
                // need to get Rope
                let rope = Rope::from_str(code.as_str());
                let url = Url::from_file_path(path).unwrap();
                // TODO: Setting version as None ( make sure this is what I want )
                // create document
                let document = document::Document::new(
                    rope.clone(),
                    Some(tree.clone()),
                    None,
                    document::FileType::Cls,
                    None,
                );
                // add document to project
                project_state.add_document(url, document);
            } else if ext == "inc" || ext == "mac" {
                // routines, use core objectscript grammar
                let code = self.read_text_async(path).await.unwrap();
                let tree = parser.parse(code.clone(), None).unwrap();
                let rope = Rope::from_str(code.as_str());
                let url = Url::from_file_path(path).unwrap();
                if ext == "inc" {
                    // TODO
                } else if ext == "mac" {
                    // TODO
                }
                let filetype = match ext {
                    "inc" => document::FileType::Inc,

                    "mac" => document::FileType::Mac,

                    _ => {
                        panic!("Unsupported file type '{}'", ext);
                    }
                };

                let document = document::Document::new(rope, Some(tree), None, filetype, None);
                project_state.add_document(url, document);
            }
        }
    }
}
