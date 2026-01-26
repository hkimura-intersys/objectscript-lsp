use crate::common::get_class_name_from_root;
use crate::parse_structures::FileType;
use crate::workspace::ProjectState;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::{Url};
use tree_sitter::Parser;
use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};
use walkdir::WalkDir;

#[derive(Debug)]
pub(crate) struct BackendTester {
    pub(crate) projects: Arc<RwLock<HashMap<Url, Arc<ProjectState>>>>,
}

impl BackendTester {
    pub(crate) fn new() -> Self {
        Self {
            projects: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) fn add_project(&self, uri: Url, state: ProjectState) {
        self.projects.write().insert(uri, Arc::new(state));
    }

    pub fn get_project(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        self.projects.read().get(uri).cloned()
    }

    fn find_parent_workspace(&self, uri: Url) -> Option<Url> {
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

    pub(crate) fn get_project_from_document_url(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        let project_url = self.find_parent_workspace(uri.clone())?;
        self.get_project(&project_url)
    }

    pub fn handle_did_open(&self, uri: Url, text: String, file_type: FileType, version: i32) {
        let Some(project) = self.get_project_from_document_url(&uri) else {
            return;
        };
        project.handle_document_opened(uri, text, file_type, version);
    }

    pub(crate) async fn index_workspace(&self, uri: &Url) {
        let Some(project) = self.get_project_from_document_url(&uri) else {
            return;
        };
        let Some(root) = project.root_path() else {
            eprintln!("Couldn't get root");
            return;
        };
        let root = root.to_path_buf();
        // Run indexing on Tokio's blocking thread pool
        let handle = tokio::task::spawn_blocking(move || {
            let mut cls_parser = Parser::new();
            if cls_parser
                .set_language(&LANGUAGE_OBJECTSCRIPT.into())
                .is_err()
            {
                eprintln!("Failed to load ObjectScript grammar");
                return;
            }

            let mut routine_parser = Parser::new();
            if routine_parser
                .set_language(&LANGUAGE_OBJECTSCRIPT_CORE.into())
                .is_err()
            {
                eprintln!("Failed to load ObjectScript Core grammar");
                return;
            }

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

                let tree = if use_core {
                    match routine_parser.parse(&code, None) {
                        Some(t) => t,
                        None => {
                            eprintln!("Failed to parse file: {:?}", path);
                            continue;
                        }
                    }
                } else {
                    match cls_parser.parse(&code, None) {
                        Some(t) => t,
                        None => {
                            eprintln!("Failed to parse file: {:?}", path);
                            continue;
                        }
                    }
                };

                // Only compute class_name for cls files; mac/inc don't have a class name.
                let class_name = if filetype == FileType::Cls {
                    get_class_name_from_root(code.as_str(), tree.root_node())
                } else {
                    Some("TODO".to_string())
                };

                let Some(class_name) = class_name else {
                    eprintln!("No class Name");
                    continue;
                };

                // Commit inside the ProjectData lock
                {
                    let mut data = project.data.write();
                    data.add_document_if_absent(url, code, tree, filetype, class_name, None);
                }
            }
            {
                let mut data = project.data.write();
                data.build_inheritance_and_variables(None);
            }
        });
        // Wait for completion (and handle join errors)
        if let Err(join_err) = handle.await {
            eprintln!("index_workspace_scope spawn_blocking failed: {join_err:?}");
        }
    }
}
