use std::collections::HashMap;
use tree_sitter::{Point, Range};
use crate::scope_tree::{Scope};
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
pub struct Class {
    // pub name: String,
    pub inherited_classes: Option<Vec<String>>,
    pub is_procedure: bool,
    pub methods: HashMap<String, Method>,
    pub range: Range, // not actually sure if range is needed for this
    pub scope: Scope,
}

pub struct ClassProperty {
    pub name: String,
    pub property_type: Option<String>,
    pub is_public: bool,
}

pub struct ClassInstance {
    pub name: String,
    pub class_name: String,
}

pub enum Method {
    ClassMethod(ClassMethod),
    InstanceMethod(InstanceMethod)
}

// each param should contain locations as well
#[derive(Clone, Debug)]
pub struct ClassMethod {
    pub return_type: Option<DataType>,
    pub name: String,
    pub range: Range,
    pub params: Option<Vec<Param>>,
    pub block: Block,
}

pub struct InstanceMethod {
    pub return_type: Option<DataType>,
    pub name: String,
    pub range: Range,
    pub params: Option<Vec<Param>>,
    pub block: Block,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub range: Range,
}

#[derive(Clone, Debug)]
pub struct Statement {

}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub range: Range,
    pub param_type: DataType
}

#[derive(Clone, Debug)]
pub enum DataType {
    // all up to tiny int are integers
    BigInt(i64), //holds a 64 bit int
    Boolean(i32), // 0 or 1
    Counter(i32),
    Integer,
    SmallInt,
    TinyInt,

    // all of these are strings
    Binary,
    Char,
    EnumString,
    ExactString,
    List,
    ListOfBinary,
    Status,
    String,
    Name,

    // date
    Date,

    // time
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
    Vector
}
