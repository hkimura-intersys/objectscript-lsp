use crate::common::initial_build_scope_tree;
use crate::parse_structures::FileType;
use crate::scope_tree::*;
use tree_sitter::Tree;

#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) content: String, // TODO: Rope provides O(log n) for text edits, insertions, and deletions compared to String's O(n) operations; might wanna use it
    pub(crate) tree: Tree,
    pub(crate) version: Option<i32>, // None if it hasn't been synced yet
    pub(crate) file_type: FileType,
    pub(crate) scope_tree: ScopeTree,
}

impl Document {
    pub fn new(content: String, tree: Tree, file_type: FileType) -> Self {
        let scope_tree = initial_build_scope_tree(tree.clone());
        Self {
            content,
            tree,
            version: None,
            file_type,
            scope_tree,
        }
    }

    fn update_scope_tree(&mut self) {
        // TODO: function that takes given scope tree, finds scope of changes, updates scope tree
    }

    pub fn update(&mut self, update_scope_tree: bool, update_tree: bool, new_tree: Option<Tree>, update_file_type: bool, new_file_type: Option<FileType>, update_version: bool, new_version: Option<i32>) {
        if update_tree {
            self.tree = new_tree.unwrap();
        }
        if update_file_type {
            self.file_type = new_file_type.unwrap();
        }
        if update_version {
            self.version = new_version;
        }
        if update_scope_tree {
            self.update_scope_tree();
        }
    }
}
