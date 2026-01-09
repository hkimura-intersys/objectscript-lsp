use crate::common::initial_build_scope_tree;
use crate::parse_structures::{ClassId, FileType, LocalSemanticModelId};
use crate::scope_tree::*;
use tree_sitter::Tree;

#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) content: String, // TODO: Rope provides O(log n) for text edits, insertions, and deletions compared to String's O(n) operations; might wanna use it
    pub(crate) tree: Tree,
    pub(crate) version: Option<i32>, // None if it hasn't been synced yet
    pub(crate) file_type: FileType,
    pub(crate) scope_tree: ScopeTree,
    pub(crate) local_semantic_model_id: Option<LocalSemanticModelId>,
    pub(crate) class_id: Option<ClassId>,
    pub(crate) class_name: String,
}

impl Document {
    pub fn new(content: String, tree: Tree, file_type: FileType, class_name: String) -> Self {
        let scope_tree = initial_build_scope_tree(tree.clone());
        Self {
            content,
            tree,
            version: None,
            file_type,
            scope_tree,
            local_semantic_model_id: None,
            class_id: None,
            class_name
        }
    }
}
