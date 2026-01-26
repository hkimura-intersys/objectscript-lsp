use std::collections::HashMap;
use std::hash::Hash;
use tree_sitter::Range;

/// Stores the Index into `GlobalSemanticModel::classes`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ClassId(pub usize);

/// Stores the Index into the per-class public method vec in `GlobalSemanticModel::methods::ClassId`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PublicMethodId(pub usize);

/// Stores the Index into `LocalSemanticModel::methods`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PrivateMethodId(pub usize);

/// Stores the Index into the per-class public variable vec in `GlobalSemanticModel::variables::ClassId`, where ClassId represents the class the variable is defined in.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PublicVarId(pub usize);

/// Stores the Index into `LocalSemanticModel::variables`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PrivateVarId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PropertyId(pub usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ParameterId(pub usize);

/// Index into `GlobalSemanticModel::private`, the vec that holds all local semantic models in a workspace.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LocalSemanticModelId(pub usize);

/// Key used to identify a method by type and name (and later, signature).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodKey {
    /// Class method or instance method.
    pub method_type: MethodType,
    /// Method name.
    pub name: String,
    // later: add signature info (arg count/types) to be correct for overloads
}

/// DFS visitation state.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DfsState {
    Unvisited,
    Visiting,
    Done,
}

/// Reference to a method implementation in a class (public or private).
///
/// Exactly one of `pub_id` or `priv_id` is expected to be `Some`, depending on visibility/type.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MethodRef {
    pub class: ClassId,
    pub pub_id: Option<PublicMethodId>,
    pub priv_id: Option<PrivateMethodId>,
}

/// Reference to a public method implementation in a class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PublicMethodRef {
    pub class: ClassId,
    pub id: PublicMethodId,
}

/// Per-document private semantic state (methods, properties, variables).
///
/// This is used for private members that should not be shared across classes globally.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSemanticModel {
    pub methods: Vec<Method>,
    pub properties: Vec<ClassProperty>,
    pub variables: Vec<Variable>,
    pub active: bool,
}

// TODO: UNIMPLEMENTED: foreignkey, relationships, storage, query, index, trigger, xdata, projection
/// Semantic representation of a parsed ObjectScript class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    /// Class Name.
    pub name: String,
    /// Imported classes referenced by this class.
    pub imports: Vec<ClassId>, // list of class names
    // format: Include (macro file name) ex: include hannah for macro file hannah.inc
    // pub include: Vec<String>, // include files are inherited by subclasses, include files bring in macros at compile time
    // pub include_gen: Vec<String>, // this specifies include files to be generated
    // if inheritance keyword == left, leftmost supersedes all (default)
    // if inheritancedirection == right, right supersedes
    /// Direct parent classes in the `Extends` list.
    pub inherited_classes: Vec<ClassId>,
    /// Inheritance conflict resolution direction (`left`, or `right`, default is `left`).
    pub inheritance_direction: String,
    /// Optional ProcedureBlock default for this class; If defined, methods will inherit this keyword if they don't specify it themselves.
    pub is_procedure_block: Option<bool>,
    /// Optional default Language keyword for this class.
    pub default_language: Option<Language>,
    /// Stores method name -> id for each private method in this class.
    pub private_methods: HashMap<String, PrivateMethodId>,
    /// Stores method name -> id for each public method in this class.
    pub public_methods: HashMap<String, PublicMethodId>,
    /// Stores property name -> id for each private property in this class.
    pub private_properties: HashMap<String, PropertyId>,
    /// Stores property name -> id for each public property in this class.
    pub public_properties: HashMap<String, PropertyId>,
    /// Stores parameter name -> id for each parameter in this class.
    pub parameters: HashMap<String, ParameterId>,
    /// Stores all method calls to external classes for this class.
    pub method_calls: Vec<MethodCallSite>,
    /// Whether this class entry is considered live/usable (e.g., false after removal).
    pub active: bool,
}

/// Language keyword values supported for classes/methods.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Language {
    Objectscript,
    TSql,
    Python,
    ISpl,
}

/// Semantic representation of a class property declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassProperty {
    pub name: String,
    pub property_type: Option<String>,
    pub is_public: bool,
    pub range: Range,
}

/// Semantic representation of a class parameter declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassParameter {
    pub name: String,
    pub property_type: Option<String>,
    pub default_argument_value: Option<String>, // this can be a numeric literal, string literal, or identifier
    pub range: Range,
}

/// Distinguishes instance methods from class methods.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MethodType {
    InstanceMethod,
    ClassMethod,
}

/// Semantic Representation of an ObjectScript Method.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method {
    /// Class Method or Instance Method.
    pub method_type: MethodType,
    /// Expected return type.
    pub return_type: Option<ReturnType>,
    /// Method Name.
    pub name: String,
    /// Stores variable name -> id for all private variables in this method.
    pub private_variables: HashMap<String, PrivateVarId>,
    /// Stores variable name -> id for all public variables in this method.
    pub public_variables: HashMap<String, PublicVarId>,
    /// Whether method is public or not.
    pub is_public: bool,
    /// Whether method is a procedure block or not. If None, method defaults to procedure block.
    pub is_procedure_block: Option<bool>,
    /// Stores language of method. If None, method defaults to ObjectScript.
    pub language: Option<Language>,
    /// Stores CodeMode of method. If None, method defaults to Code.
    pub code_mode: CodeMode,
    /// Names declared in `PublicList(...)` of ProcedureBlocks.
    pub public_variables_declared: Vec<String>,
}

/// CodeMode keyword values supported for methods.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodeMode {
    Call,
    Code,
    Expression,
    ObjectGenerator,
}

/// Parsed representation of a class method call expression (syntactic/semantic summary).
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
/// Parsed representation of an OREF (object reference) method call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrefMethodCall {
    pub name: String,
    pub class_name: String,
    pub method_name: String,
    pub is_public: bool,
}

/// Parsed representation of an OREF creation site (e.g., `##class(X).%New()` assignment).
/// Example: set person=##class(Sample.Person).%New()
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Oref {
    pub name: String,
    pub class_name: String,
    pub is_public: bool,
}

/// Semantic representation of a variable discovered in a method.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// Optional type of the argument if the variable originated from a method argument.
    pub arg_type: Option<ReturnType>,
    /// Types discovered from the RHS expression that defines/assigns this variable.
    pub var_type: Vec<VarType>,
    /// Whether variable is public or not.
    pub is_public: bool,
}

/// Normalized return/type categories recognized.
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

/// Normalized expression atom categories used to classify RHS variable types/dependencies.
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
    InstanceVariable(String),
    RelativeDotProperty,
    OrefChainExpr, // not sure if this is ever a method
    // references to parameters
    RelativeDotParameter,
    ClassParameterRef,
    // other
    SystemDefined,
    DollarSf,
    ExtrinsicFunction,
    Gvn(String),
    Lvn(String),
    Glvn(String),
    Other(String),
}

/// File type for a workspace document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileType {
    Cls,
    Mac,
    Inc,
}

/// Unresolved method call site extracted from a method body.
///
/// Stores the textual callee class/method plus source ranges; resolution to symbols happens later.
#[derive(Clone, Debug)]
pub struct UnresolvedCallSite {
    /// Name of Class being called (e.g. `"Foo.Bar"`).
    pub callee_class: String,
    /// Name of method being called (e.g. `"Baz"`).
    pub callee_method: String,
    /// Range covering the method call.
    pub call_range: Range,
    /// Ranges for each argument expression.
    pub arg_ranges: Vec<Range>,
}

/// Semantic Representation of a Method Call to an External Method (any method defined outside of the given Class)
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallSite {
    /// Name of the method containing the method call.
    pub caller_method: String,
    /// Name of Class being called (e.g. `"Foo.Bar"`).
    pub callee_class: String,
    /// Name of method being called (e.g. `"Baz"`).
    pub callee_method: String,
    /// Resolved public method reference if known; otherwise `None`.
    pub callee_symbol: Option<PublicMethodRef>,
    /// Range covering the method call.
    pub call_range: Range,
    /// Ranges for each argument expression.
    pub arg_ranges: Vec<Range>,
}
