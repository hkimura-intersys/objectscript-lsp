use crate::common::{find_return_type, generic_exit_statements, generic_skipping_statements, get_node_children, get_string_at_byte_range, start_of_function, successful_exit};
use crate::parse_structures::{CodeMode, Language, Method, MethodType, ReturnType, Variable};
use crate::variable::{build_variable_from_argument, build_variable_from_set_argument_rhs};
use std::collections::HashMap;
use tree_sitter::{Node, Range};
use crate::common;

/// Builds a `Method` from its header/definition node (first-pass parse).
///
/// Parses the method name, return type, and method keywords (ProcedureBlock/Language/CodeMode,
/// visibility, and public variable list). Does **not** parse the method body statements; those
/// are handled in a later pass.
///
/// Returns the constructed `Method` and the source `Range` for the definition node.
pub fn initial_build_method(
    node: Node,
    method_type: MethodType,
    content: &str,
) -> Option<(Method, Range)> {
    start_of_function("COMMON: no struct", "initial_build_method");
    let Some(method_name_node) = node.named_child(0) else {
        eprintln!("Couldn't get given Node's child at index 0");
        generic_exit_statements("COMMON: no struct", "initial_build_method");
        return None;
    };
    let Some(method_name) = get_string_at_byte_range(content, method_name_node.byte_range()) else {
        eprintln!("Couldn't get the method name string..");
        generic_exit_statements("COMMON: no struct", "initial_build_method");
        return None;
    };
    let method_range = node.range();
    let mut method_return_type = None;
    let mut is_procedure_block = None;
    let mut language = None;
    let mut codemode = None;
    let mut is_public = true;
    let mut public_variables = Vec::new();
    let children = get_node_children(node.clone());
    for node in children[1..].iter() {
        match node.kind() {
            "return_type" => {
                let Some(type_name_node) = node.named_child(1) else {
                    eprintln!("Warning: couldn't get return type node ({:?}) child at index 1", node);
                    generic_skipping_statements("initial_build_method", "Node", "node");
                    continue;
                };
                let Some(typename) = get_string_at_byte_range(content, type_name_node.byte_range())
                else {
                    eprintln!("Warning: Failed to get the string for return type name node {:?}", type_name_node);
                    generic_skipping_statements("initial_build_method", "Node", "node");
                    continue;
                };
                method_return_type = find_return_type(typename);
            }
            "method_keywords" => {
                let Some((
                    is_procedure_block_val,
                    language_val,
                    codemode_val,
                    is_public_val,
                    public_variables_val,
                )) = common::handle_method_keywords(node.clone(), content)
                else {
                    eprintln!("warning: handle method keywords returned None for method keywords node: {:?}", node);
                    generic_skipping_statements("initial_build_method", "Node", "node");
                    continue;
                };
                is_procedure_block = is_procedure_block_val;
                language = language_val;
                codemode = codemode_val;
                is_public = is_public_val;
                public_variables = public_variables_val;
            }
            _ => {
                eprintln!("Info: Initial build only parses method header definition, not block");
                generic_skipping_statements("initial_build_method", "Node", "node");
                continue;
            }
        }
    }
    let method = Method::new(
        method_name,
        is_procedure_block,
        language,
        codemode.unwrap_or(CodeMode::Code),
        is_public,
        method_return_type,
        public_variables,
        method_type,
    );
    successful_exit("COMMON: No struct", "initial_build_method");
    Some((method, method_range))
}

impl Method {
    /// Creates a new `Method` from parsed header information.
    ///
    /// Initializes empty variable tables and stores declared keywords/visibility/type metadata.
    pub fn new(
        method_name: String,
        is_procedure_block: Option<bool>,
        language: Option<Language>,
        code_mode: CodeMode,
        is_public: bool,
        return_type: Option<ReturnType>,
        public_variables: Vec<String>,
        method_type: MethodType,
    ) -> Self {
        Self {
            method_type,
            return_type,
            name: method_name,
            private_variables: HashMap::new(),
            public_variables: HashMap::new(),
            is_public,
            is_procedure_block,
            language,
            code_mode,
            public_variables_declared: public_variables,
        }
    }

    /// Parses a method definition node to extract variables and their dependencies.
    ///
    /// Collects:
    /// - argument variables from the `arguments` node
    /// - variables assigned via `set` statements in the core body
    ///
    /// Returns a list of `(variable, definition_range, var_dependencies, property_dependencies)`.
    /// Visibility (public vs private) is inferred from ProcedureBlock and `public_variables_declared`.
    pub fn build_method_variables_and_ref(
        &self,
        node: Node,
        content: &str,
    ) -> Vec<(Variable, Range, Vec<String>, Vec<String>)> {
        start_of_function("Method", "build_method_variables_and_ref");
        let mut variables: Vec<(Variable, Range, Vec<String>, Vec<String>)> = Vec::new();
        let children = get_node_children(node.clone());
        for node in children.iter().skip(1) {
            if node.kind() == "arguments" {
                let children = get_node_children(node.clone());
                for node in children {
                    // each node is an argument (aka variable)
                    let Some(var_name) = node
                        .named_child(0)
                        .and_then(|n| content.get(n.byte_range()))
                        .map(str::to_string)
                    else {
                        eprintln!("Failed to get var name from node");
                        generic_skipping_statements("build_method_variables_and_ref", "Node", "node");
                        continue;
                    };
                    if self.is_procedure_block.unwrap_or(true) == false
                        || self.public_variables_declared.contains(&var_name)
                    {
                        variables.push(build_variable_from_argument(node, var_name, content, true));
                    } else {
                        variables
                            .push(build_variable_from_argument(node, var_name, content, false));
                    }
                }
            } else if node.kind() == "core_method_body_content" {
                let children = get_node_children(node.clone());
                for statement in children {
                    let Some(node) = statement.named_child(0) else {
                        eprintln!(
                            "Couldn't get statement node child at index 0, statement: {:?}",
                            statement
                        );
                        generic_skipping_statements("build_method_variables_and_ref", "Method Statement in core body", "node");
                        continue;
                    }; // actual command
                    match node.kind() {
                        "command_set" => {
                            let Some(set_argument) = node.named_child(1) else {
                                eprintln!("Warning: failed to get set argument node (index 1) from command_set node");
                                generic_skipping_statements("build_method_variables_and_ref", "Set command node", "node");
                                continue;
                            };
                            let Some(var_name) = set_argument
                                .named_child(0)
                                .and_then(|n| content.get(n.byte_range()))
                                .map(str::to_string)
                            else {
                                eprintln!("Warning: failed to get set argument child node (index 1) from set_argument node");
                                generic_skipping_statements("build_method_variables_and_ref", "set argument node", "node");
                                continue;
                            };

                            let Some(set_argument_child) = set_argument.named_child(1) else {
                                eprintln!(
                                    "Warning: failed to get set argument child node (index 1) from set_argument node"
                                );
                                generic_skipping_statements("build_method_variables_and_ref", "set argument node", "node");
                                continue;
                            };
                            if self.is_procedure_block.unwrap_or(true) == false
                                || self.public_variables_declared.contains(&var_name)
                            {
                                variables.push(build_variable_from_set_argument_rhs(
                                    set_argument_child,
                                    var_name,
                                    content,
                                    true,
                                ));
                            } else {
                                variables.push(build_variable_from_set_argument_rhs(
                                    set_argument_child,
                                    var_name,
                                    content,
                                    false,
                                ));
                            }
                        }
                        _ => {
                            eprintln!("Warning: Statement {:?} not yet implemented", node);
                            generic_skipping_statements("build_method_variables_and_ref", "Statement", "node");
                            continue;
                        }
                    }
                }
            }
        }
        successful_exit("Method", "build_method_variables_and_ref");
        variables
    }

    /// Applies inherited class keywords to this method when not explicitly set.
    ///
    /// - Inherits `ProcedureBlock=false` only when the method has no explicit setting.
    /// - Inherits the class `default_language` when the method language is unset.
    pub fn update_keywords(&mut self, is_procedure_block: bool, default_language: Language) {
        start_of_function("Method", "update_keywords");
        // inherit class keywords if not specified and class keyword isn't the default value
        if self.is_procedure_block.is_none() && is_procedure_block == false {
            // inherit the class keyword when it isn't the default
            self.is_procedure_block = Some(is_procedure_block);
        }

        if self.language.is_none() {
            // inherit the class keyword when it isn't the default
            self.language = Some(default_language.clone());
        }
        successful_exit("Method", "update_keywords");
    }
}
