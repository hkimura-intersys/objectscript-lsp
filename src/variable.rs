use crate::common::{
    find_return_type, find_var_type_from_expression, generic_skipping_statements,
    get_node_children, get_string_at_byte_range, start_of_function, successful_exit,
};
use crate::parse_structures::{ReturnType, VarType, Variable};
use tree_sitter::{Node, Range};

/// Build a `Variable` from the RHS expression of a `set` argument.
///
/// This inspects the expression to infer `VarType`s (via `find_var_type_from_expression`) and also
/// returns dependency lists:
/// - `var_refs`: referenced variable names
/// - `property_refs`: referenced instance properties
///
/// Returns a tuple of:
/// `(variable, rhs_range, var_refs, property_refs)`.
pub fn build_variable_from_set_argument_rhs(
    node: Node,
    var_name: String,
    content: &str,
    is_public: bool,
    var_range: Range,
) -> (Variable, Range, Vec<String>, Vec<String>) {
    start_of_function(
        "Building Variable (No Struct)",
        "build_variable_from_set_argument_rhs",
    );
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
    successful_exit(
        "Building Variable (No Struct)",
        "build_variable_from_set_argument_rhs",
    );

    (
        Variable::new(var_name, None, argument_value, is_public),
        var_range,
        var_refs,
        property_refs,
    )
}

/// Parse a method argument node into a `Variable`.
///
/// Extracts an optional declared argument type and an optional default value. If the default value
/// is an `expression`, this function also infers `VarType`s and collects dependency lists:
/// - `var_refs`: referenced variable names (GVN/LVN/GLVN)
/// - `property_refs`: referenced instance properties
///
/// Returns a tuple of:
/// `(variable, arg_range, var_refs, property_refs)`.
pub fn build_variable_from_argument(
    node: Node,
    var_name: String,
    content: &str,
    is_public: bool,
    var_name_range: Range,
) -> (Variable, Range, Vec<String>, Vec<String>) {
    start_of_function(
        "Building Variable (No Struct)",
        "build_variable_from_argument",
    );
    let children = get_node_children(node.clone());
    let mut argument_type = None;
    let mut argument_value: Vec<VarType> = Vec::new();
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
                } else {
                    eprintln!(
                        "Warning: failed to get string for type name node. Node is {:?}",
                        type_name_node
                    );
                    generic_skipping_statements(
                        "build_variable_from_argument",
                        type_name_node.kind(),
                        "Node",
                    );
                    continue;
                }
            } else {
                eprintln!("Warning: failed to get node child at index 1, expected type name node to be there. Node (argument_type) is {:?}", node);
                generic_skipping_statements("build_variable_from_argument", node.kind(), "Node");
                continue;
            }
        } else if node.kind() == "default_argument_value" {
            let Some(arg_content_node) = node.named_child(0) else {
                eprintln!("Warning: failed to get node child at index 0, expected arg value to be there. Node (default_argument_value) is {:?}", node);
                generic_skipping_statements("build_variable_from_argument", node.kind(), "Node");
                continue;
            };
            let Some(arg_content) =
                get_string_at_byte_range(content, arg_content_node.byte_range())
            else {
                eprintln!(
                    "Warning: failed to get string for method arg value node. Node is {:?}",
                    arg_content_node
                );
                generic_skipping_statements(
                    "build_variable_from_argument",
                    arg_content_node.kind(),
                    "Node",
                );
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
                            generic_skipping_statements(
                                "build_variable_from_argument",
                                arg_content_node.kind(),
                                "Node",
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
                        generic_skipping_statements(
                            "build_variable_from_argument",
                            arg_content_node.kind(),
                            "Node",
                        );
                        continue;
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
                    eprintln!("Unexpected Method Arg Value {:?}", arg_content_node.kind());
                    generic_skipping_statements(
                        "build_variable_from_argument",
                        arg_content_node.kind(),
                        "Node",
                    );
                    continue;
                }
            }
        }
    }
    successful_exit(
        "Building Variable (No Struct)",
        "build_variable_from_argument",
    );
    (
        Variable::new(var_name, argument_type, argument_value, is_public),
        var_name_range,
        var_refs,
        property_refs,
    )
}

impl Variable {
    /// Construct a `Variable` with an optional declared argument type and inferred expression types.
    ///
    /// `arg_type` is typically set for method arguments, while `var_type` represents the inferred
    /// types/atoms observed in the RHS/default expression.
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
