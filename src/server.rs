use crate::common::{
    generic_exit_statements, generic_skipping_statements, get_class_name_from_root,
    start_of_function, successful_exit,
};
use crate::parse_structures::FileType;
use crate::workspace::ProjectState;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::{MessageType, Url};
use tower_lsp::Client;
use tree_sitter::Parser;
use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};
use walkdir::WalkDir;

pub struct BackendWrapper(pub(crate) Arc<Backend>);
impl BackendWrapper {
    /// Create a reference-counted backend wrapper around a new `Backend`.
    pub fn new(client: Client) -> Self {
        Self(Arc::new(Backend::new(client)))
    }
}
pub(crate) struct Backend {
    /// LSP Client.
    pub(crate) client: Client,
    /// Stores Url -> ProjectState for each Workspace.
    pub(crate) projects: Arc<RwLock<HashMap<Url, Arc<ProjectState>>>>,
}

impl Backend {
    /// Construct a new backend with an empty projects map.
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            projects: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a workspace (project) and its initial `ProjectState` by workspace URI.
    pub(crate) fn add_project(&self, uri: Url, state: ProjectState) {
        // start_of_function("Backend", "add_project");
        self.projects.write().insert(uri, Arc::new(state));
        // successful_exit("Backend", "add_project");
    }

    /// Fetch a project by its workspace URI.
    ///
    /// Returns a cloned `Arc` to the project state, or `None` if the workspace is not registered.
    pub fn get_project(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        // start_of_function("Backend", "get_project");
        let result = self.projects.read().get(uri).cloned();
        // successful_exit("Backend", "get_project");
        result
    }

    /// Find the workspace URI that most specifically contains the given document URI.
    ///
    /// Converts the document URI to a file path and selects the registered workspace whose path is
    /// the longest prefix of that document path (i.e., the deepest matching workspace).
    fn find_parent_workspace(&self, uri: Url) -> Option<Url> {
        // start_of_function("Backend", "find_parent_workspace");
        let doc_path: PathBuf = uri.to_file_path().ok()?;

        // find longest prefix
        let projects = self.projects.read();

        let parent = projects
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
            .map(|(_, ws_uri)| ws_uri);
        // successful_exit("Backend", "find_parent_workspace");
        parent
    }

    /// Resolve the `ProjectState` associated with a document URI.
    ///
    /// This first finds the containing workspace (if any), then returns that project's state.
    pub(crate) fn get_project_from_document_url(&self, uri: &Url) -> Option<Arc<ProjectState>> {
        // start_of_function("Backend", "get_project_from_document_url");
        let project_url = self.find_parent_workspace(uri.clone())?;
        let result = self.get_project(&project_url);
        // successful_exit("Backend", "get_project_from_document_url");
        result
    }

    /// Handle an LSP "didOpen" for a document by forwarding it to the owning project.
    ///
    /// If no workspace contains `uri`, this is a no-op.
    pub fn handle_did_open(&self, uri: Url, text: String, file_type: FileType, version: i32) {
        // start_of_function("Backend", "handle_did_open");
        let Some(project) = self.get_project_from_document_url(&uri) else {
            return;
        };
        project.handle_document_opened(uri, text, file_type, version);
        // successful_exit("Backend", "handle_did_open");
    }

    /// Index all `.cls`, `.mac`, and `.inc` files under the workspace root containing `uri`.
    ///
    /// This runs filesystem walking and parsing on Tokio's blocking thread pool. Each file is read,
    /// parsed with the appropriate Tree-sitter grammar, and inserted into the project's document
    /// store if absent. After the scan, inheritance and variable information is built once.
    pub(crate) async fn index_workspace(&self, uri: &Url) {
        start_of_function("Backend", "index_workspace");
        let Some(project) = self.get_project_from_document_url(&uri) else {
            eprintln!(
                "Failed to get project from document with url: {:?}",
                uri.path()
            );
            generic_exit_statements("Backend", "index_workspace");
            return;
        };
        let Some(root) = project.root_path() else {
            self.client
                .log_message(MessageType::ERROR, "project root path doesn't exist")
                .await;
            generic_exit_statements("Backend", "index_workspace");
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
                generic_exit_statements("Backend", "index_workspace");
                return;
            }

            let mut routine_parser = Parser::new();
            if routine_parser
                .set_language(&LANGUAGE_OBJECTSCRIPT_CORE.into())
                .is_err()
            {
                eprintln!("Failed to load ObjectScript Core grammar");
                generic_exit_statements("Backend", "index_workspace");
                return;
            }

            let mut documents_already_existing = Vec::new();
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
                    Err(_) => {
                        eprintln!("Error: Failed to read file contents: {}", path.display());
                        let Some(path_as_str) = path.as_os_str().to_str() else {
                            generic_skipping_statements(
                                "index_workspace",
                                "Couldn't get path str",
                                "File Contents",
                            );
                            continue;
                        };
                        generic_skipping_statements(
                            "index_workspace",
                            path_as_str,
                            "File contents for the following path",
                        );
                        continue;
                    }
                };

                let url = match Url::from_file_path(path) {
                    Ok(u) => u,
                    Err(_) => {
                        eprintln!("Error: Failed to convert path to Url: {}", path.display());
                        let Some(path_as_str) = path.as_os_str().to_str() else {
                            generic_skipping_statements(
                                "index_workspace",
                                "Couldn't get path str",
                                "Path",
                            );
                            continue;
                        };
                        generic_skipping_statements("index_workspace", path_as_str, "path");
                        continue;
                    }
                };

                let tree = if use_core {
                    match routine_parser.parse(&code, None) {
                        Some(t) => t,
                        None => {
                            eprintln!("Failed to parse file: {:?}", path);
                            generic_skipping_statements(
                                "index_workspace",
                                code.as_str(),
                                "File contents",
                            );
                            continue;
                        }
                    }
                } else {
                    match cls_parser.parse(&code, None) {
                        Some(t) => t,
                        None => {
                            eprintln!("Failed to parse file: {:?}", path);
                            generic_skipping_statements(
                                "index_workspace",
                                code.as_str(),
                                "File contents",
                            );
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
                    let already_exists = data.add_document_if_absent(
                        url.clone(),
                        code,
                        tree,
                        filetype,
                        class_name,
                        None,
                    );
                    if already_exists {
                        documents_already_existing.push(url);
                    }
                }
            }
            {
                let mut data = project.data.write();
                data.build_inheritance_and_variables(None, documents_already_existing);
            }
        });
        // Wait for completion (and handle join errors)
        if let Err(join_err) = handle.await {
            eprintln!("index_workspace_scope spawn_blocking failed: {join_err:?}");
            generic_exit_statements("Backend", "index_workspace");
        }
        successful_exit("Backend", "index_workspace");
    }
}
