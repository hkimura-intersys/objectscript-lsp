use std::collections::HashMap;
use tower_lsp::lsp_types::Url;
use tree_sitter::{Point, Range};

#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodSymbolId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VariableSymbolId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VariableGlobalSymbolId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ClassGlobalSymbolId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodGlobalSymbolId(pub usize);

#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub(crate) start: Point, // have to convert to Position for ls client
    pub(crate) end: Point,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) method_symbols: Vec<MethodSymbol>,
    pub(crate) variable_symbols: Vec<VariableSymbol>,
    pub(crate) public_var_defs: HashMap<String, VariableGlobalSymbolId>,
    pub(crate) private_variable_defs: HashMap<String, (ScopeId, VariableSymbolId)>,
    pub(crate) is_new_scope: bool, // this is for legacy code only new a,b should give a syntax error for cls files
}

#[derive(Clone, Debug)]
pub struct VariableGlobalSymbol {
    pub name: String,
    pub url: Url,
    pub location: Range,
    pub var_dependencies: Vec<String>,
    pub property_dependencies: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MethodGlobalSymbol {
    pub name: String,
    pub url: Url,
    pub location: Range,
}

#[derive(Clone, Debug)]
pub struct ClassGlobalSymbol {
    pub name: String,
    pub url: Url,
    pub location: Range,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub struct VariableSymbol {
    pub name: String,
    pub location: Range,
    pub scope_id: ScopeId,
    pub references: Vec<Range>,
    pub var_dependencies: Vec<String>,
    pub property_dependencies: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MethodSymbol {
    pub name: String,
    pub location: Range,
    pub scope_id: ScopeId,
}
