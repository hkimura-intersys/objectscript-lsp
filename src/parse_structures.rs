use std::collections::HashMap;
use tree_sitter::{Range};
use crate::scope_tree::{ScopeId};
use tower_lsp::lsp_types::Url;
/*
SEMANTIC CHECKS:
1. If the class instance is calling a class that DOESN'T extend either %Persistent, %SerialObject, or %RegisteredObject, fail
2. Can't have two classes or methods that are named the same thing:
ClassMethod Install() As %Status {

    }
ClassMethod Install(gatewayName As %String,offline As %Boolean = 0) As %Status

Should fail

NICE THINGS TO HAVE:
1. have a var name light up if it is ever used, and have it dim otherwise. This makes it so someone can see if their var is never used.
 */

// TODO : I want a function that gets a scope given a method name

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VarId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PropertyId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ParameterId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Clone, Debug)]
pub enum SymbolKind {
    Class(String), // might not need this, but curr set up to pass in class name
    Method(MethodId),
    PubVar(VarId),
    PrivVar(VarId),
    ClassParameter(ParameterId),
    ClassProperty(PropertyId),
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Range,
    pub scope: ScopeId,
    pub references: Vec<Range>,
}


// TODO: UNIMPLEMENTED: foreignkey, relationships, storage, query, index, trigger, xdata, projection
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    pub name: String,
    pub imports: Vec<String>, // list of class names
    // format: Include (macro file name) ex: include hannah for macro file hannah.inc
    pub include: Vec<String>, // include files are inherited by subclasses, include files bring in macros at compile time
    pub include_gen: Vec<String>, // this specifies include files to be generated
    // if inheritance keyword == left, leftmost supersedes all (default)
    // if inheritancedirection == right, right supersedes
    pub inherited_classes: Vec<String>,
    pub inheritance_direction: String,
    pub is_procedure_block: Option<bool>,
    pub default_language: Option<Language>,
    // method name -> methodId
    pub methods: HashMap<String, MethodId>,
    pub public_variables: HashMap<String, VarId>,
    pub properties: HashMap<String, PropertyId>,
    pub parameters: HashMap<String, ParameterId>,
    pub scope: ScopeId,
    // pub subclasses: Vec<String> // class names
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Language {
    Objectscript,
    TSql,
    Python,
    ISpl,
}

impl Class {
    pub fn new(name: String, scope: ScopeId, imports: Vec<String>, include: Vec<String>,  include_gen: Vec<String>) -> Self {
        Self {
            name,
            imports,
            include,
            include_gen,
            inherited_classes: Vec::new(),
            inheritance_direction: "left".to_string(),
            is_procedure_block: None,
            default_language: None,
            methods: HashMap::new(),
            public_variables: HashMap::new(),
            properties: HashMap::new(),
            parameters: HashMap::new(),
            scope,

        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassProperty {
    pub name: String,
    pub property_type: Option<String>,
    pub is_public: bool,
    pub range: Range,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassParameter {
    pub name: String,
    pub property_type: Option<String>,
    pub default_argument_value: Option<String>, // this can be a numeric literal, string literal, or identifier
    pub range: Range,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MethodType {
    InstanceMethod,
    ClassMethod
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method {
    pub method_type: MethodType,
    pub return_type: Option<VarType>,
    pub name: String,
    pub priv_vars: HashMap<String,VarId>,
    pub scope: ScopeId,
    pub is_public: bool,
    pub is_procedure_block: bool,
}

impl Method {
    pub fn new(name: String, method_type: MethodType, scope:ScopeId) -> Self {
        Self {
            method_type,
            return_type: None,
            name,
            priv_vars: HashMap::new(),
            scope,
            is_public:true,
            is_procedure_block:true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassMethodCall {
    pub name: String,
    pub class_name: String,
    pub is_public: bool,
}

/// TODO: IMPORTANT: For validating oref methods,
/// if the method doesn't exist, we need to check
/// the properties. It may be a subscript into a
/// property
///
/// This struct should be used for values where a
/// known oref (Registered object) is calling a
/// method
///
/// TODO: debating if i should store the method name
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrefMethodCall {
    pub name: String,
    pub class_name: String,
    pub is_public: bool,
}

/// This represents the class method call
/// that actually creates an OREF:
/// Example: set person=##class(Sample.Person).%New()
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Oref {
    pub name: String,
    pub class_name: String,
    pub is_public: bool,
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variable {
    pub name: String,
    pub var_type: VarType,
    pub is_public: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VarType {
    JsonObjectLiteral,
    JsonArrayLiteral,
    Macro,
    String,
    Number,
    Oref, // special type of class method call
    // potential references to methods
    RelativeDotMethod,
    OrefMethodCall, // special type of orefchainexpr
    ClassMethodCall,
    SuperclassMethodCall,
    // references to properties
    InstanceVariable,
    RelativeDotProperty,
    OrefChainExpr, // not sure if this is ever a method
    // references to parameters
    RelativeDotParameter,
    ClassParameterRef,
    // other
    SystemDefined,
    DollarSf,
    ExtrinsicFunction,
    Other,
}

impl Variable {
    pub fn new(name: String, var_type: VarType, is_public: bool) -> Self {
        Self {
            name,
            var_type,
            is_public,
        }
    }
}

#[derive(Clone, Debug)]
pub enum FileType {
    Cls,
    Mac,
    Inc,
}

