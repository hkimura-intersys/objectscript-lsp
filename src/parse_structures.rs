use std::collections::HashMap;
use std::hash::Hash;
use tree_sitter::Range;
use url::Url;
use crate::scope_structures::{GlobalSymbol};
/*
SEMANTIC CHECKS:
1. If the class instance is calling a class that DOESN'T extend either %Persistent, %SerialObject,
or %RegisteredObject, fail
2. Can't have two classes or methods that are named the same thing:
ClassMethod Install() As %Status {

    }
ClassMethod Install(gatewayName As %String,offline As %Boolean = 0) As %Status

Should fail

NICE THINGS TO HAVE:
1. have a var name light up if it is ever used, and have it dim otherwise. This makes it so someone
can see if their var is never used.
*/

// TODO : I want a function that gets a scope given a method name
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ClassId(pub usize);
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VarId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PropertyId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ParameterId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalSemanticModelId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MethodHandle {
    Global(MethodId),
    Local(LocalSemanticModelId, MethodId),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodKey {
    pub method_type: MethodType,   // ClassMethod vs Method
    pub name: String,
    // later: add signature info (arg count/types) to be correct for overloads
}

pub struct OverrideIndex {
    pub overrides: HashMap<MethodHandle, MethodHandle>,             // child -> base
    pub overridden_by: HashMap<MethodHandle, Vec<MethodHandle>>,    // base  -> children
}



#[derive(Clone, Debug)]
pub struct GlobalSemanticModel {
    pub variables: Vec<Variable>,
    pub classes: Vec<Class>,
    pub methods: Vec<Method>,
    pub private: Vec<LocalSemanticModel>,
    pub(crate) defs: Vec<GlobalSymbol>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSemanticModel {
    pub methods: Vec<Method>,
    pub properties: Vec<ClassProperty>,
    pub variables: Vec<Variable>,
}

/*
What I need:
Classes: Url -> ClassId (OR class name -> ClassId)
variables: (var name, var type) -> Vec<Variables> -> there are multiple because it can be defined diff in diff places if its public
 */
// TODO: UNIMPLEMENTED: foreignkey, relationships, storage, query, index, trigger, xdata, projection
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    pub name: String,
    pub imports: Vec<ClassId>, // list of class names
    // format: Include (macro file name) ex: include hannah for macro file hannah.inc
    // pub include: Vec<String>, // include files are inherited by subclasses, include files bring in macros at compile time
    // pub include_gen: Vec<String>, // this specifies include files to be generated
    // if inheritance keyword == left, leftmost supersedes all (default)
    // if inheritancedirection == right, right supersedes
    pub inherited_classes: Vec<ClassId>,
    pub inheritance_direction: String,
    pub is_procedure_block: Option<bool>,
    pub default_language: Option<Language>,
    // method name -> methodId
    // private methods/properties are stored in local semantic model
    // public methods/properties are stored in global semantic model
    pub private_methods: HashMap<String, MethodId>,
    pub public_methods: HashMap<String, MethodId>,
    pub inherited_methods: HashMap<String, MethodId>,
    pub private_properties: HashMap<String, PropertyId>,
    pub public_properties: HashMap<String, PropertyId>,
    pub parameters: HashMap<String, ParameterId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Language {
    Objectscript,
    TSql,
    Python,
    ISpl,
}

#[derive(Clone, Debug)]
pub struct GlobalVarRef {
    pub url: Url,
    pub var_id: VarId,
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

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MethodType {
    InstanceMethod,
    ClassMethod,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method {
    pub method_type: MethodType,
    pub return_type: Option<ReturnType>,
    pub name: String,
    pub variables: HashMap<String, VarId>,
    pub is_public: bool,
    pub is_procedure_block: Option<bool>,
    pub language: Option<Language>,
    pub code_mode: CodeMode,
    // vec of variable names for public variables declared
    pub public_variables_declared: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodeMode {
    Call,
    Code,
    Expression,
    ObjectGenerator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassMethodCall {
    pub name: String,
    pub class_name: String,
    pub method_name: String,
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
    pub method_name: String,
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
    pub arg_type: Option<ReturnType>,
    pub var_type: Option<VarType>,
    pub is_public: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReturnType {
    String,
    Integer,
    TinyInteger, // has diff max and min values
    Number,
    Binary,
    Decimal,
    Boolean,
    Date,
    Status,
    TimeStamp,
    DynamicObject,
    DynamicArray,
    Float,
    Double,
    HttpResponse,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VarType {
    JsonObjectLiteral, // Dynamic Object
    JsonArrayLiteral,  // Dynamic Array
    Macro,
    String,
    Number,
    RoutineTagCall,
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
    Other(String),
}

#[derive(Clone, Debug)]
pub enum FileType {
    Cls,
    Mac,
    Inc,
}
