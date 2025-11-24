use crate::document::Document;
use crate::scope_tree::{ScopeTree};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock};
use tower_lsp::lsp_types::Url;
use tree_sitter::{Node, Point};
// TODO: switch project documents back to some, remove option
// TODO: fix build_scope_tree

// pub static PUBLIC_METHODS
// class name -> methods available (how would this deal with instance methods tho...)
// need to distinguish between pub local vars and global vars in the case of new commands in routines
pub static PUBLIC_LOCAL_VARIABLE_DEFS: LazyLock<Arc<RwLock<HashMap<String,Vec<(Url,Point)>>>>> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));
pub static GLOBAL_VARIABLE_DEFS: LazyLock<Arc<RwLock<HashMap<String, Vec<(Url, Point)>>>>> = LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));
pub(crate) struct ProjectState {
    pub project_root_path: OnceLock<Option<PathBuf>>, //should only ever be set on initialize()
    pub documents: Arc<RwLock<HashMap<Url, Document>>>,
    pub defs: Arc<RwLock<HashMap<Url, ScopeTree>>>,

    // // need to differentiate between these in the case of New (globals don't get redeclared)
    // pub public_local_defs: Arc<RwLock<HashMap<String,Vec<(Url,Point)>>>>,
    // pub global_defs: Arc<RwLock<HashMap<String, Vec<(Url, Point)>>>>,
}

// helper function to recursively walk tree sitter parsed tree
pub fn walk_tree<F>(node: Node, callback: &mut F)
where
    F: FnMut(Node),
{
    callback(node);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(child, callback);
    }
}

pub fn cls_is_scope_node(node: Node) -> bool {
    if node.kind() == "classmethod" || node.kind() == "method" {
        return true;
    }
    false
}

impl ProjectState {
    pub(crate) fn new() -> Self {
        Self {
            project_root_path: OnceLock::new(),
            documents: Arc::new(RwLock::new(HashMap::new())),
            defs: Arc::new(RwLock::new(HashMap::new())),
            // public_local_defs: Arc::new(RwLock::new(HashMap::new())),
            // global_defs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn routine_build_scope_tree(&self, url:Url) {
        /*
        if node.kind() == "command_new" {
                // FOR CLS FILES, SYNTAX ERR IF THIS IS ANYTHING BESIDES THE ESTACK, ETRAP, NAMESPACE, and roles
                let current_scope_id = *scope_stack.last().unwrap();
                let current_scope = scope_tree.scopes.read().get(&current_scope_id).unwrap().clone();
                let mut current_scope_defs = current_scope.defs.clone();
                let current_scope_end = current_scope.end.clone();

                let new_args = scope_tree.get_new_command_args(node);
                for arg in new_args {
                    current_scope_defs.remove(&arg);
                }

                 let scope_id = scope_tree.add_scope(
                     node.end_position(),
                     current_scope_end.clone(),
                     current_scope_id,
                     Some(current_scope_defs),
                     true
                 );
                scope_stack.push(scope_id);
            }
         */
    }

    /// Creates a new nested Scope in the ScopeTree if the node is a scope node
    fn cls_build_scope_tree(&self, url: Url)  {
        let tree = {
            let documents = self.documents.read();
            let document = documents.get(&url).unwrap();
            document
                .tree
                .clone()
                .expect("Failed to get tree from document")
        };

        let content = {
            let documents = self.documents.read();
            let document = documents.get(&url).unwrap();
            document.content.clone().to_string()
        };

        let mut scope_tree = ScopeTree::new(content);
        let mut scope_stack = vec![scope_tree.root];

        walk_tree(tree.root_node(), &mut |node| {
            if cls_is_scope_node(node) {
                let scope_id = scope_tree.add_scope(
                    node.start_position(),
                    node.end_position(),
                    *scope_stack.last().unwrap(),
                    None,
                    false
                );
                scope_stack.push(scope_id);
            }

            if node.kind() == "command_new" {
                let new_args = scope_tree.get_new_command_args(node);
                for arg in new_args {
                    if arg.to_lowercase().as_str() != "$namespace"
                        && arg.to_lowercase().as_str() != "$etrap"
                        && arg.to_lowercase().as_str() != "estack"
                        && arg.to_lowercase().as_str() != "roles"
                    {
                        panic!("For cls files, the only acceptable args for new are $ESTACK, $ETRAP, $NAMESPACE or $ROLES, not: {}", arg);
                    }
                }
            }
        });
        self.defs.write().insert(url, scope_tree);
    }
    pub fn add_document(&self, url: Url, document: Document) {
        self.documents.write().insert(url, document);
    }

    pub fn root_path(&self) -> Option<&std::path::Path> {
        self.project_root_path.get().and_then(|o| o.as_deref())
    }
}
