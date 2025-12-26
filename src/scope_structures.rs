use crate::parse_structures::{PrivateVarId, PublicVarId};
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;
use tree_sitter::{Point, Range};

#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct GlobalSymbolId(pub usize);

#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub(crate) start: Point, // have to convert to Position for ls client
    pub(crate) end: Point,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) public_var_defs: HashMap<String, GlobalSymbolId>,
    pub(crate) is_new_scope: bool, // this is for legacy code only new a,b should give a syntax error for cls files
}

#[derive(Clone, Debug)]
pub enum SymbolKind {
    Method,
    PrivVar,
    // ClassProperty(PropertyId),
}

#[derive(Clone, Debug)]
pub enum GlobalSymbolKind {
    Class,
    Method,
    PubVar,
    // ClassParameter(ParameterId),
    // ClassProperty(PropertyId),
}

pub struct GlobalVarRef {
    var_id: PublicVarId,
    dependencies: Vec<String>, // variable names
}

pub struct PrivateVarRef {
    var_id: PrivateVarId,
    dependencies: Vec<String>, // variable names
}
#[derive(Clone, Debug)]
pub struct GlobalSymbol {
    pub name: String,
    pub kind: GlobalSymbolKind,
    pub url: Url,
    pub location: Range,
    pub var_dependencies: Vec<String>,
    pub property_dependencies: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Range,
    pub scope: ScopeId,
    pub references: Vec<Range>,
    pub var_dependencies: Vec<String>,
    pub property_dependencies: Vec<String>,
}
