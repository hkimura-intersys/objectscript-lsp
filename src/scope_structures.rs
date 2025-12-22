use std::collections::HashMap;
use tree_sitter::{Point, Range};
use tower_lsp::lsp_types::Url;
use crate::parse_structures::{ClassId, MethodId, ParameterId, PropertyId, VarId};

#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub(crate) start: Point, // have to convert to Position for ls client
    pub(crate) end: Point,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) defs: HashMap<String, SymbolId>, // only will store the original def, not redefs
    pub(crate) refs: HashMap<String, Vec<Range>>,
    pub(crate) is_new_scope: bool, // this is for legacy code only new a,b should give a syntax error for cls files
}

#[derive(Clone, Debug)]
pub enum SymbolKind {
    Method(MethodId),
    PrivVar(VarId),
    ClassProperty(PropertyId),
}

#[derive(Clone, Debug)]
pub enum GlobalSymbolKind {
    Class(ClassId), // might not need this, but curr set up to pass in class name
    Method(MethodId),
    PubVar(VarId),
    ClassParameter(ParameterId),
    ClassProperty(PropertyId),
}

// Should be (name, kind) -> GlobalSymbol
// note that there is no point in storing references outside of the url,
// because we don't know if they are referring to this one or a different one
// therefore, references only includes references from the same url. TODO: maybe it should be also same scope
// TODO: for now, I am just now including references
#[derive(Clone, Debug)]
pub struct GlobalSymbol {
    pub name: String,
    pub kind: GlobalSymbolKind,
    pub url: Url,
    pub location: Range,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Range,
    pub scope: ScopeId,
    pub references: Vec<Range>,
}
