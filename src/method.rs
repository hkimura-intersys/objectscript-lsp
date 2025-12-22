use crate::common::{find_return_type, get_keyword, get_node_children};
use crate::parse_structures::{CodeMode, Language, Method, MethodType, ReturnType};
use std::collections::HashMap;
use tree_sitter::{Node, Range};

/*
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
    pub public_variables_list: Vec<String>
}
 */

/// given a method_keywords node
pub(crate) fn handle_method_keywords(
    node: Node,
    content: &str,
) -> (
    Option<bool>,
    Option<Language>,
    Option<CodeMode>,
    bool,
    Vec<String>,
) {
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
        let keyword = node.named_child(0).unwrap();
        if keyword.kind() == procedure_block {
            if is_procedure_block.is_some() {
                panic!(
                    "Procedure block keyword has already been set as {:?} for this method.",
                    is_procedure_block.unwrap()
                );
            }
            let children = get_node_children(keyword.clone());
            if children.len() == 1 {
                is_procedure_block = Some(true);
            } else {
                let keyword_rhs = content[children[1].byte_range()].to_string();
                match keyword_rhs.as_str() {
                    "0" => {
                        is_procedure_block = Some(false);
                    }
                    "1" => {
                        is_procedure_block = Some(true);
                    }
                    _ => {
                        panic!(
                            "Invalid boolean Value for ProcedureBlock keyword: {}",
                            keyword_rhs
                        );
                    }
                }
            }
        } else if keyword.kind() == call_codemode_keyword {
            if codemode.is_some() {
                panic!("CodeMode is already set as {:?}", codemode);
            }
            codemode = Some(CodeMode::Call);
        } else if keyword.kind() == expression_codemode_keyword {
            if codemode.is_some() {
                panic!("CodeMode is already set as {:?}", codemode);
            }
            codemode = Some(CodeMode::Expression);
        } else if keyword.kind() == codemode_keyword {
            if codemode.is_some() {
                panic!("CodeMode is already set as {:?}", codemode);
            }
            if content[keyword.named_child(1).unwrap().byte_range()]
                .to_string()
                .to_lowercase()
                == "code"
            {
                codemode = Some(CodeMode::Code);
            } else if content[keyword.named_child(1).unwrap().byte_range()]
                .to_string()
                .to_lowercase()
                == "objectgenerator"
            {
                codemode = Some(CodeMode::ObjectGenerator);
            }
        } else if keyword.kind() == external_language_keyword {
            if language.is_some() {
                panic!("Language is already set as {:?} for this method", language);
            }
            if content[keyword.named_child(1).unwrap().byte_range()]
                .to_string()
                .to_lowercase()
                == "tsql"
            {
                language = Some(Language::TSql);
                // self.class.default_language = Some(Language::TSql);
            } else if content[keyword.named_child(1).unwrap().byte_range()]
                .to_string()
                .to_lowercase()
                == "python"
            {
                language = Some(Language::Python);
                // self.class.default_language = Some(Language::Python);
            } else if content[keyword.named_child(1).unwrap().byte_range()]
                .to_string()
                .to_lowercase()
                == "ispl"
            {
                language = Some(Language::ISpl);
                // self.class.default_language = Some(Language::ISpl);
            } else {
                println!("KEYWORD {:?}", content[keyword.byte_range()].to_string());
                println!(
                    "LANGUAGE SPECIFIED IS NOT ALLOWED {:?}",
                    content[keyword.named_child(1).unwrap().byte_range()].to_string()
                )
            }
        } else if keyword.kind() == objectscript_language_keyword {
            if language.is_some() {
                panic!("Language is already set as {:?} for this method", language);
            }
            language = Some(Language::Objectscript);
            // self.class.default_language = Some(Language::Objectscript);
        } else if keyword.kind() == private_keyword {
            is_public = false;
        } else if keyword.kind() == public_var_list {
            let children = get_node_children(keyword.clone());
            for node in children[1..].iter() {
                public_variables.push(content[node.byte_range()].to_string());
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
    (
        is_procedure_block,
        language,
        codemode,
        is_public,
        public_variables,
    )
}

// /// given an argument node, create a variable
// pub(crate) fn handle_method_argument(node: Node, content: &str) -> Variable {
//     let children = get_node_children(node.clone());
//     let var_name = content[node.named_child(0).unwrap().byte_range()].to_string();
//     let mut argument_type = None;
//     let mut argument_value = None;
//     // each node is an argument
//     for node in children[1..].iter() {
//         if node.kind() == "argument_type" {
//             let typename = content[node.named_child(1).unwrap().byte_range()].to_string();
//             argument_type = find_return_type(typename);
//         } else if node.kind() == "default_argument_value" {
//             let node = node.named_child(0).unwrap();
//             let arg_content = content[node.byte_range()].to_string();
//             match node.kind() {
//                 "string_literal" => {
//                     if argument_type.is_some() && argument_type != Some(ReturnType::String) {
//                         panic!("default_argument_value ({:?}) is a string, but specified type ({:?}) is not", arg_content, argument_type.unwrap());
//                     }
//                     argument_value = Some(VarType::String);
//                 }
//                 "numeric_literal" => {
//                     if argument_type.is_some()
//                         && argument_type != Some(ReturnType::Number)
//                         && argument_type != Some(ReturnType::Integer)
//                         && argument_type != Some(ReturnType::TinyInteger)
//                     {
//                         panic!("default_argument_value ({:?}) is a number, but specified type ({:?}) is not", arg_content, argument_type.unwrap());
//                     }
//                     argument_value = Some(VarType::Number);
//                 }
//                 "expression" => {
//                     argument_value = find_var_type_from_expression(node.clone());
//                 }
//                 _ => {
//                     panic!("Unexpected Method Arg Value {:?}", node.kind())
//                 }
//             }
//         }
//     }
//     Variable {
//         name: var_name,
//         arg_type: argument_type,
//         var_type: argument_value,
//         is_public: false,
//     }
// }

/// Note that this build does not include any statements in the method block or method arguments;
/// those will happen on the second iteration.
pub fn initial_build_method(node: Node, method_type: MethodType, content: &str) -> (Method, Range) {
    let method_name = content[node.child(0).unwrap().byte_range()].to_string();
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
            // "arguments" => {
            //     let children = get_node_children(node.clone());
            //     for node in children {
            //         // each node is an argument
            //         let arg_name =
            //             content[node.named_child(0).unwrap().byte_range()].to_string();
            //         let variable = handle_method_argument(node, content);
            //         // let var_id = self.local_semantic_model.new_variable(variable);
            //         // method_variables.insert(arg_name, var_id);
            //     }
            // }
            "return_type" => {
                let typename = content[node.named_child(1).unwrap().byte_range()].to_string();
                method_return_type = find_return_type(typename);
            }
            "method_keywords" => {
                let results: (
                    Option<bool>,
                    Option<Language>,
                    Option<CodeMode>,
                    bool,
                    Vec<String>,
                ) = handle_method_keywords(node.clone(), content);
                is_procedure_block = results.0;
                language = results.1;
                codemode = results.2;
                is_public = results.3;
                public_variables = results.4;
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
    (method, method_range)
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
            variables: HashMap::new(),
            is_public,
            is_procedure_block,
            language,
            code_mode,
            public_variables_declared: public_variables,
        }
    }
}
