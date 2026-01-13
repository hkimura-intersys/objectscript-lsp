use crate::common::{find_return_type, get_keyword, get_node_children, get_string_at_byte_range};
use crate::parse_structures::{CodeMode, Language, Method, MethodType, ReturnType, Variable};
use crate::variable::{build_variable_from_argument, build_variable_from_set_argument_rhs};
use std::collections::HashMap;
use tree_sitter::{Node, Range};

#[derive(Clone, Debug)]
pub struct UnresolvedCallSite {
    pub callee_class: String,
    pub callee_method: String,
    pub call_range: Range, // range of the call node (class_method_call or relative_dot_method)
    pub arg_ranges: Vec<Range>, // range of each arg expression
}

/// Given a Method Definition Node, find all class and instance method calls.
pub fn build_method_calls(
    current_class: &str,
    method_definition_node: Node,
    content: &str,
) -> Vec<UnresolvedCallSite> {
    let mut out = Vec::new();

    let children = get_node_children(method_definition_node);
    for child in children.into_iter().skip(1) {
        if child.kind() != "core_method_body_content" {
            continue;
        }

        // each child is a statement
        for statement in get_node_children(child) {
            let Some(cmd) = statement.named_child(0) else {
                continue;
            };

            match cmd.kind() {
                "command_do" => {
                    let Some(do_arg) = cmd.named_child(1) else {
                        continue;
                    };

                    match do_arg.kind() {
                        "class_method_call" => {
                            //  child(0): class_ref
                            //  child(1): method name
                            //  child(2): argument list node
                            let call_range = do_arg.range();

                            let Some(class_ref) = do_arg.named_child(0) else {
                                continue;
                            };
                            let class_ref_name = {
                                let Some(name_node) = class_ref.named_child(1) else {
                                    continue;
                                };
                                let Some(s) =
                                    get_string_at_byte_range(content, name_node.byte_range())
                                else {
                                    continue;
                                };
                                s
                            };

                            let callee_method = {
                                let Some(m) = do_arg.named_child(1) else {
                                    continue;
                                };
                                let Some(s) = get_string_at_byte_range(content, m.byte_range())
                                else {
                                    continue;
                                };
                                s
                            };

                            let arg_ranges: Vec<Range> = do_arg
                                .named_child(2)
                                .map(|args_node| {
                                    get_node_children(args_node)
                                        .into_iter()
                                        .map(|a| a.range())
                                        .collect()
                                })
                                .unwrap_or_else(Vec::new);

                            out.push(UnresolvedCallSite {
                                callee_class: class_ref_name,
                                callee_method,
                                call_range,
                                arg_ranges,
                            });
                        }

                        "instance_method_call" => {
                            // only handle relative-dot method calls with no chains for now for simplicity
                            let parts = get_node_children(do_arg);
                            if parts.len() != 1 {
                                continue;
                            }
                            let rel = parts[0];
                            if rel.kind() != "relative_dot_method" {
                                continue;
                            }

                            let call_range = rel.range();

                            // oref_method node in your earlier code
                            let Some(oref_method) = rel.named_child(0) else {
                                continue;
                            };

                            let callee_method = {
                                let Some(m) = oref_method.named_child(0) else {
                                    continue;
                                };
                                let Some(s) = get_string_at_byte_range(content, m.byte_range())
                                else {
                                    continue;
                                };
                                s
                            };

                            let arg_ranges: Vec<Range> = oref_method
                                .named_child(1)
                                .map(|args_node| {
                                    get_node_children(args_node)
                                        .into_iter()
                                        .map(|a| a.range())
                                        .collect()
                                })
                                .unwrap_or_else(Vec::new);

                            out.push(UnresolvedCallSite {
                                callee_class: current_class.to_string(),
                                callee_method,
                                call_range,
                                arg_ranges,
                            });
                        }

                        _ => {
                            // ignore other DO forms for now
                        }
                    }
                }

                "command_job" => {
                    // TODO: implement job statement parsing similarly
                }

                _ => {}
            }
        }
    }
    out
}

/// given a method_keywords node
pub(crate) fn handle_method_keywords(
    node: Node,
    content: &str,
) -> Option<(
    Option<bool>,
    Option<Language>,
    Option<CodeMode>,
    bool,
    Vec<String>,
)> {
    let mut is_procedure_block: Option<bool> = None;
    let mut is_public = true;
    let mut public_variables = Vec::new();
    let method_keywords_children = get_node_children(node.clone());
    let procedure_block = get_keyword("method_keyword", "procedure");
    let private_keyword = get_keyword("method_keyword", "private");
    let public_var_list = get_keyword("method_keyword", "public_list");
    let objectscript_language_keyword = get_keyword("method_keyword", "language");
    let external_language_keyword = "method_keyword_language".to_string();
    // regular codemode (core)
    let codemode_keyword = get_keyword("method_keyword", "codemode");
    // expression code mode (expression method)
    let expression_codemode_keyword = "method_keyword_codemode_expression".to_string();
    let call_codemode_keyword = "call_method_keyword".to_string();
    let mut codemode: Option<CodeMode> = None;
    let mut language: Option<Language> = None;
    // each node here is a class_keyword
    for node in method_keywords_children.iter() {
        let Some(keyword) = node.named_child(0) else {
            continue;
        };
        if keyword.kind() == procedure_block {
            if is_procedure_block.is_some() {
                // TODO: LOG ERROR Procedure block keyword has already been set as {:?} for this method.
                return None;
            }
            let children = get_node_children(keyword.clone());
            if children.len() == 1 {
                is_procedure_block = Some(true);
            } else {
                let Some(rhs_keyword_node) = children.get(1) else {
                    continue;
                };
                let Some(keyword_rhs) =
                    get_string_at_byte_range(content, rhs_keyword_node.byte_range())
                else {
                    continue;
                };
                match keyword_rhs.as_str() {
                    "0" => {
                        is_procedure_block = Some(false);
                    }
                    "1" => {
                        is_procedure_block = Some(true);
                    }
                    _ => {
                        // TODO: LOG ERROR
                        return None;
                        // panic!(
                        //     "Invalid boolean Value for ProcedureBlock keyword: {}",
                        //     keyword_rhs
                        // );
                    }
                }
            }
        } else if keyword.kind() == call_codemode_keyword {
            if codemode.is_some() {
                // TODO: LOG ERROR
                return None;
                // panic!("CodeMode is already set as {:?}", codemode);
            }
            codemode = Some(CodeMode::Call);
        } else if keyword.kind() == expression_codemode_keyword {
            if codemode.is_some() {
                // TODO: LOG ERROR
                return None;
                // panic!("CodeMode is already set as {:?}", codemode);
            }
            codemode = Some(CodeMode::Expression);
        } else if keyword.kind() == codemode_keyword {
            if codemode.is_some() {
                // TODO: LOG ERROR
                return None;
                // panic!("CodeMode is already set as {:?}", codemode);
            }
            if let Some(value_node) = keyword.named_child(1) {
                if let Some(text) = content.get(value_node.byte_range()) {
                    if text.eq_ignore_ascii_case("code") {
                        codemode = Some(CodeMode::Code);
                    } else if text.eq_ignore_ascii_case("objectgenerator") {
                        codemode = Some(CodeMode::ObjectGenerator);
                    }
                }
            }
        } else if keyword.kind() == external_language_keyword {
            if language.is_some() {
                // TODO: LOG ERROR
                return None;
                // panic!("Language is already set as {:?} for this method", language);
            }
            if let Some(value_node) = keyword.named_child(1) {
                if let Some(text) = content.get(value_node.byte_range()) {
                    if text.eq_ignore_ascii_case("tsql") {
                        language = Some(Language::TSql);
                    } else if text.eq_ignore_ascii_case("python") {
                        language = Some(Language::Python);
                    } else if text.eq_ignore_ascii_case("ispl") {
                        language = Some(Language::ISpl);
                    } else {
                        // TODO: LOG ERROR
                        return None;
                    }
                }
            }
        } else if keyword.kind() == objectscript_language_keyword {
            if language.is_some() {
                // TODO: LOG ERROR
                // panic!("Language is already set as {:?} for this method", language);
            }
            language = Some(Language::Objectscript);
            // self.class.default_language = Some(Language::Objectscript);
        } else if keyword.kind() == private_keyword {
            is_public = false;
        } else if keyword.kind() == public_var_list {
            let children = get_node_children(keyword.clone());
            for node in children[1..].iter() {
                if let Some(text) = content.get(node.byte_range()) {
                    public_variables.push(text.to_string());
                }
            }
        }
    }
    if codemode.is_none() {
        codemode = Some(CodeMode::Code);
    }
    /*
    TODO: check class keywords after the initial build (so not in this part), after classes inherit
    keywords as well
    */
    Some((
        is_procedure_block,
        language,
        codemode,
        is_public,
        public_variables,
    ))
}

/// Note that this build does not include any statements in the method block or method arguments;
/// those will happen on the second iteration.
pub fn initial_build_method(
    node: Node,
    method_type: MethodType,
    content: &str,
) -> Option<(Method, Range)> {
    let Some(method_name_node) = node.named_child(0) else {
        eprintln!("Couldn't get given Node's child at index 0");
        return None;
    };
    let Some(method_name) = get_string_at_byte_range(content, method_name_node.byte_range()) else {
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
                    eprintln!("Couldn't get given Node: {:?} child at index 0", node);
                    return None;
                };
                let Some(typename) = get_string_at_byte_range(content, type_name_node.byte_range())
                else {
                    return None;
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
                         )) = handle_method_keywords(node.clone(), content)
                else {
                    continue;
                };
                is_procedure_block = is_procedure_block_val;
                language = language_val;
                codemode = codemode_val;
                is_public = is_public_val;
                public_variables = public_variables_val;
            }
            _ => {
                println!("Initial build only parses method header definition, not block")
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
    Some((method, method_range))
}

impl Method {
    /// given a method_definition node, create the initial build of a Method.
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

    /// Given a method definition node, build the statements in the method body semantically.
    /// This includes variables, method arguments, and do/job statements
    pub fn build_method_variables_and_ref(
        &self,
        node: Node,
        content: &str,
    ) -> Vec<(Variable, Range, Vec<String>, Vec<String>)> {
        let mut variables: Vec<(Variable, Range, Vec<String>, Vec<String>)> = Vec::new();
        let children = get_node_children(node.clone());
        for node in children[1..].iter() {
            if node.kind() == "arguments" {
                let children = get_node_children(node.clone());
                for node in children {
                    // each node is an argument (aka variable)
                    let Some(var_name) = node
                        .named_child(0)
                        .and_then(|n| content.get(n.byte_range()))
                        .map(str::to_string)
                    else {
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
                // each child is a statement
                for statement in children {
                    let Some(node) = statement.named_child(0) else {
                        eprintln!("Couldn't get statement node child at index 0");
                        continue;
                    }; // actual command
                    match node.kind() {
                        "command_set" => {
                            let Some(set_argument) = node.named_child(1) else {
                                eprintln!("Couldn't get set argument node child");
                                continue;
                            };
                            let Some(var_name) = set_argument
                                .named_child(0)
                                .and_then(|n| content.get(n.byte_range()))
                                .map(str::to_string)
                            else {
                                eprintln!("In set command, failed to get variable name");
                                continue;
                            };

                            let Some(set_argument_child) = set_argument.named_child(1) else {
                                eprintln!(
                                    "In set command, failed to get set argument node's child"
                                );
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
                            println!("Statement {:?} not yet implemented", node);
                            return variables;
                        }
                    }
                }
            }
        }
        variables
    }
}
