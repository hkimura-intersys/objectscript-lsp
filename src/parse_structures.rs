use std::collections::HashMap;
use tree_sitter::{Point, Range};
use crate::scope_tree::{ScopeId};
use tower_lsp::lsp_types::{Url};
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
pub struct GlobalDefs {

}

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
    pub range: Range,
    pub scope: ScopeId,
    pub references: Vec<Range>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    pub name: String,
    pub inherited_class: Option<String>,
    pub is_procedure_block: bool,
    // method name -> methodId
    pub class_methods: HashMap<String, MethodId>,
    pub instance_methods : HashMap<String, MethodId>,
    pub properties: HashMap<String, PropertyId>,
    pub parameters: HashMap<String, ParameterId>,
    pub range: Range, // not actually sure if range is needed for this
    pub scope: ScopeId,
    pub subclasses: Vec<String> // class name
}

impl Class {
    pub fn new(name: String, range: Range, scope: ScopeId) -> Self {
        Self {
            name,
            inherited_class: None,
            is_procedure_block: true,
            class_methods: HashMap::new(),
            instance_methods: HashMap::new(),
            properties: HashMap::new(),
            parameters: HashMap::new(),
            range,
            scope,
            subclasses: Vec::new()
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
    pub return_type: Option<DataType>,
    pub name: String,
    pub range: Range,
    pub pub_vars : HashMap<String,VarId>,
    pub priv_vars: HashMap<String,VarId>,
    pub scope: ScopeId,
    pub is_public: bool,
    pub is_procedure_block: bool,
}

impl Method {
    pub fn new(name: String, range: Range, method_type: MethodType,scope:ScopeId) -> Self {
        Self {
            method_type,
            return_type: None,
            name,
            range,
            pub_vars: HashMap::new(),
            priv_vars: HashMap::new(),
            scope,
            is_public:true,
            is_procedure_block:true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstanceMethod {
    pub return_type: Option<DataType>,
    pub name: String,
    pub range: Range,
    pub pub_vars : HashMap<String,VarId>,
    pub priv_vars: HashMap<String,VarId>,
    pub block_scope: ScopeId,
    pub is_public: bool,
    pub is_procedure_block: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Var {
    MethodParameter(MethodParameter),
    ClassInstance(ClassInstance),
    Variable(Variable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodParameter {
    pub name: String,
    pub range: Range,
    pub param_type: DataType
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassInstance {
    pub name: String,
    pub class_name: String,
    pub is_public: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub var_type: Option<DataType>,
    pub range: Range,
    pub is_public: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataType {
    // all up to tiny int are integers
    BigInt,
    Boolean,
    Counter,
    Integer,
    SmallInt,
    TinyInt,
    Binary,
    Char,
    EnumString,
    ExactString,
    List,
    ListOfBinary,
    Status,
    String,
    Name,
    Date,
    Time,

    // numeric
    Decimal,
    Numeric,
    Currency,

    //double
    Double,

    // timestamp
    DateTime,
    TimeStamp,
    PosixTime,

    // vector
    Vector,
    Class(String), // a class type
}
