use crate::common::{get_keyword, get_node_children};
use crate::method::initial_build_method;
use crate::parse_structures::{Class, Language, Method, MethodType};
use std::collections::HashMap;
use tree_sitter::{Node, Range};
/*
For Simplicity, I am not including the logic for parsing
include or include_gen files. This feature can be added later.
 */
impl Class {
    pub fn new(name: String) -> Self {
        Self {
            name,
            imports: Vec::new(),
            inherited_classes: Vec::new(),
            inheritance_direction: "left".to_string(),
            is_procedure_block: None,
            default_language: None,
            private_methods: HashMap::new(),
            public_methods: HashMap::new(),
            private_properties: HashMap::new(),
            public_properties: HashMap::new(),
            parameters: HashMap::new(),
        }
    }

    /// Starting from the source_file node, build the initial
    /// semantic representation of an objectscript class
    /// This does not include inherited classes, imports, or include files.
    /// Those will be handled in the second iteration of parsing.
    pub fn initial_build(&mut self, node: Node, content: &str) -> Vec<(Method, Range)> {
        let class_children = get_node_children(node);
        let mut methods = Vec::new();
        // skip keyword_class and class_name
        for node in class_children[2..].iter() {
            match node.kind() {
                "class_keywords" => {
                    self.initial_build_class_keywords(node.clone(), content);
                }
                "class_body" => {
                    let children = get_node_children(node.clone());
                    // each child is a class statement
                    for child in children {
                        let method = self.handle_class_statement_method(child, content);
                        if method.is_some() {
                            methods.push(method.unwrap());
                        }
                    }
                }
                _ => {
                    println!("Unimplemented class child {}", node.kind())
                }
            }
        }
        methods
    }

    /// given a class_statement node, build the corresponding statement struct
    fn handle_class_statement_method(
        &mut self,
        node: Node,
        content: &str,
    ) -> Option<(Method, Range)> {
        let statement_type = node.named_child(0).unwrap();
        let statement_definition = statement_type.named_child(1).unwrap();
        match statement_type.kind() {
            "method" => {
                let (method, range) =
                    initial_build_method(statement_definition, MethodType::InstanceMethod, content);
                Some((method, range))
            }
            "classmethod" => {
                let (method, range) =
                    initial_build_method(statement_definition, MethodType::ClassMethod, content);
                Some((method, range))
            }
            _ => {
                println!(
                    "Unimplementated class statement {:?}",
                    statement_type.kind()
                );
                None
            }
        }
    }

    /// Given the class_keywords node, parses out the
    /// keywords to find if there is one for ProcedureBlock
    /// or for Language. If so, adjusts the class structure accordingly.
    fn initial_build_class_keywords(&mut self, node: Node, content: &str) {
        let class_keywords_children = get_node_children(node.clone());
        let procedure_block = get_keyword("class_keyword", "procedure");
        let language_keyword = get_keyword("class_keyword", "language");
        let inheritance_keyword = get_keyword("class_keyword", "inheritance");
        // each node here is a class_keyword
        for node in class_keywords_children.iter() {
            let keyword = node.named_child(0).unwrap();
            if keyword.kind() == procedure_block {
                if keyword.named_child(0).unwrap().kind() == "keyword_not" {
                    self.is_procedure_block = Some(false);
                } else {
                    self.is_procedure_block = Some(true);
                }
            } else if keyword.kind() == language_keyword {
                if content[keyword.named_child(1).unwrap().byte_range()]
                    .to_string()
                    .to_lowercase()
                    == "tsql"
                {
                    self.default_language = Some(Language::TSql);
                } else if content[keyword.named_child(1).unwrap().byte_range()]
                    .to_string()
                    .to_lowercase()
                    == "objectscript"
                {
                    self.default_language = Some(Language::Objectscript);
                } else {
                    println!("KEYWORD {:?}", content[keyword.byte_range()].to_string());
                    println!(
                        "LANGUAGE SPECIFIED IS NOT ALLOWED {:?}",
                        content[keyword.named_child(1).unwrap().byte_range()]
                            .to_string()
                            .to_lowercase()
                    )
                }
            } else if keyword.kind() == inheritance_keyword {
                if content[keyword.named_child(1).unwrap().byte_range()]
                    .to_string()
                    .to_lowercase()
                    == "right"
                {
                    self.inheritance_direction = "right".to_string();
                }
            }
        }
    }

    //
    //
    // /// this includes class keyword inheritance, method inheritance
    // /// Second iteration build must pass in the root node, as we want to
    // /// handle imports as well
    // fn second_iteration_build(&mut self, node:Node, content: &str) {
    //     // handle imports
    // }
}
