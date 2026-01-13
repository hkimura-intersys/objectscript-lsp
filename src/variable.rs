use crate::common::{
    find_return_type, find_var_type_from_expression, get_node_children, get_string_at_byte_range,
};
use crate::parse_structures::{ReturnType, VarType, Variable};
use tree_sitter::{Node, Range};

/// Given the lhs of a set argument, build a Variable
pub fn build_variable_from_set_argument_rhs(
    node: Node,
    var_name: String,
    content: &str,
    is_public: bool,
) -> (Variable, Range, Vec<String>, Vec<String>) {
    let var_range = node.range();
    let mut var_refs = Vec::new();
    let mut property_refs = Vec::new();

    let argument_value = find_var_type_from_expression(node.clone(), content);
    for val in argument_value.clone() {
        if let VarType::Gvn(var_name) = val {
            var_refs.push(var_name);
        } else if let VarType::Lvn(var_name) = val {
            var_refs.push(var_name);
        } else if let VarType::Glvn(var_name) = val {
            var_refs.push(var_name);
        } else if let VarType::InstanceVariable(property_name) = val {
            property_refs.push(property_name);
        }
    }

    (
        Variable::new(var_name, None, argument_value, is_public),
        var_range,
        var_refs,
        property_refs,
    )
}
/// parses an argument node into a variable. Sets privacy based on method keywords.
pub fn build_variable_from_argument(
    node: Node,
    var_name: String,
    content: &str,
    is_public: bool,
) -> (Variable, Range, Vec<String>, Vec<String>) {
    let children = get_node_children(node.clone());
    let mut argument_type = None;
    let mut argument_value: Vec<VarType> = Vec::new();
    let var_range = node.range();
    let mut var_refs = Vec::new();
    let mut property_refs = Vec::new();
    // each node is an argument
    for node in children[1..].iter() {
        if node.kind() == "argument_type" {
            if let Some(type_name_node) = node.named_child(1) {
                if let Some(typename) =
                    get_string_at_byte_range(content, type_name_node.byte_range())
                {
                    argument_type = find_return_type(typename);
                }
            }
        } else if node.kind() == "default_argument_value" {
            let Some(arg_content_node) = node.named_child(0) else {
                eprintln!("Couldn't get default argument value child");
                continue;
            };
            let Some(arg_content) =
                get_string_at_byte_range(content, arg_content_node.byte_range())
            else {
                eprintln!("Couldn't get default argument value child string content");
                continue;
            };
            match arg_content_node.kind() {
                "string_literal" => {
                    if let Some(arg) = argument_type.as_ref() {
                        if *arg != ReturnType::String {
                            eprintln!(
                                "default_argument_value ({:?}) is a string, but specified type ({:?}) is not",
                                arg_content, arg
                            );
                            continue;
                        }
                    }
                    argument_value.push(VarType::String);
                }
                "numeric_literal" => {
                    if argument_type.is_some()
                        && argument_type != Some(ReturnType::Number)
                        && argument_type != Some(ReturnType::Integer)
                        && argument_type != Some(ReturnType::TinyInteger)
                    {
                        eprintln!("default_argument_value is a number, but specified type is not");
                    }
                    argument_value.push(VarType::Number);
                }
                "expression" => {
                    argument_value = find_var_type_from_expression(node.clone(), content);
                    for val in argument_value.clone() {
                        if let VarType::Gvn(var_name) = val {
                            var_refs.push(var_name);
                        } else if let VarType::Lvn(var_name) = val {
                            var_refs.push(var_name);
                        } else if let VarType::Glvn(var_name) = val {
                            var_refs.push(var_name);
                        } else if let VarType::InstanceVariable(property_name) = val {
                            property_refs.push(property_name);
                        }
                    }
                }
                _ => {
                    eprintln!("Unexpected Method Arg Value {:?}", arg_content_node.kind())
                }
            }
        }
    }
    (
        Variable::new(var_name, argument_type, argument_value, is_public),
        var_range,
        var_refs,
        property_refs,
    )
}

impl Variable {
    pub fn new(
        var_name: String,
        arg_type: Option<ReturnType>,
        var_type: Vec<VarType>,
        is_public: bool,
    ) -> Self {
        Self {
            name: var_name,
            arg_type,
            var_type,
            is_public,
        }
    }
}
