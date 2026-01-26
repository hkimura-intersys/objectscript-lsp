use tower_lsp::lsp_types::Url;
use tree_sitter::Range;

/// The Key into `ScopeTree::scopes` representing a single `Scope`.
#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(pub usize);

/// Stores the index into `Scope::variable_symbols`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VariableSymbolId(pub usize);

/// Stores the index into the per-class public variable symbol vec in `GlobalSemanticModel::variable_defs::ClassGlobalSymbolId`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VariableGlobalSymbolId(pub usize);

/// Stores the index into `GlobalSemanticModel::class_defs`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ClassGlobalSymbolId(pub usize);

/// Stores the index into the per-class public method symbol vec in `GlobalSemanticModel::method_defs::ClassGlobalSymbolId`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodGlobalSymbolId(pub usize);

/// A variable definition symbol for a public variable (definition site + dependency metadata).
#[derive(Clone, Debug)]
pub struct VariableGlobalSymbol {
    /// Variable name.
    pub name: String,
    /// Document URl containing the variable definition.
    pub url: Url,
    /// Source range of the variable definition.
    pub location: Range,
    /// Names of other variables referenced by this definition.
    pub var_dependencies: Vec<String>,
    /// Names of properties referenced by this definition.
    pub property_dependencies: Vec<String>,
}

/// A method definition symbol for a public method.
#[derive(Clone, Debug)]
pub struct MethodGlobalSymbol {
    /// Method Name
    pub name: String,
    /// Document URl containing the method definition.
    pub url: Url,
    /// Source range of the method definition.
    pub location: Range,
}

/// A class definition symbol (definition site + liveness flag).
#[derive(Clone, Debug)]
pub struct ClassGlobalSymbol {
    /// Class Name
    pub name: String,
    /// Document URl containing the class definition.
    pub url: Url,
    /// Source range of the class definition.
    pub location: Range,
    /// Whether this symbol currently represents a live document/class (false after removal).
    pub alive: bool,
}

/// A private variable symbol (definition + references + dependency metadata).
#[derive(Clone, Debug)]
pub struct VariableSymbol {
    /// Variable name.
    pub name: String,
    /// Source range of the variable definition.
    pub location: Range,
    /// Source ranges of references/uses associated with this symbol.
    pub references: Vec<Range>,
    /// Names of other variables referenced by this definition.
    pub var_dependencies: Vec<String>,
    /// Names of properties referenced by this definition.
    pub property_dependencies: Vec<String>,
}

/// A Private Method Symbol
#[derive(Clone, Debug)]
pub struct MethodSymbol {
    /// Method name
    pub name: String,
    /// Source range of the method definition.
    pub location: Range,
}
