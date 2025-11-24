use crate::scope_tree::*;
use ropey::Rope;
use tree_sitter::{Tree, Range};

pub enum FileType {
    Cls,
    Mac,
    Inc,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Clone, Debug)]
pub enum SymbolKind {
    Class,
    Method,
    Var,
    Property,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub scope: ScopeId,          // from your ScopeTree
    pub references: Vec<Range>,  // optional but very useful
}

pub struct SemanticModel {
    pub scope_tree: ScopeTree,
    pub symbols: Vec<Symbol>,
    // later: pub classes: Vec<Class>, methods: Vec<Method>, vars: Vec<Variable>, ...
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
