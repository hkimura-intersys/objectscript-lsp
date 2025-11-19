use crate::scope_tree::*;
use ropey::Rope;
use tree_sitter::Tree;

pub enum FileType {
    Cls,
    Mac,
    Inc,
}

pub(crate) struct Document {
    pub(crate) content: Rope, // provides O(log n) for text edits, insertions, and deletions compared to String's O(n) operations
    pub(crate) tree: Option<Tree>,
    version: Option<i32>, // None if it hasn't been synced yet
    file_type: FileType,
    pub(crate) scope_tree: Option<ScopeTree>,
}

impl Document {
    pub(crate) fn new(
        content: Rope,
        tree: Option<Tree>,
        version: Option<i32>,
        file_type: FileType,
        scope_tree: Option<ScopeTree>,
    ) -> Self {
        Self {
            content,
            tree,
            version,
            file_type,
            scope_tree,
        }
    }
}
