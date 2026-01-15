use crate::common::{get_keyword, get_node_children, get_string_at_byte_range};
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
            method_calls: Vec::new(),
            active: true,
        }
    }

    pub fn clear(&mut self, class_name: String, active: bool) {
        self.name = class_name;
        self.imports = Vec::new();
        self.inherited_classes = Vec::new();
        self.inheritance_direction = "left".to_string();
        self.is_procedure_block = None;
        self.default_language = None;
        self.private_methods = HashMap::new();
        self.public_methods = HashMap::new();
        self.private_properties = HashMap::new();
        self.public_properties = HashMap::new();
        self.parameters = HashMap::new();
        self.method_calls = Vec::new();
        self.active = active;
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
                        let Some((method, method_range)) =
                            self.handle_class_statement_method(child, content)
                        else {
                            eprintln!("Failed to get method from handle_class_statement_method");
                            continue;
                        };

                        methods.push((method, method_range));
                    }
                }
                _ => {
                    eprintln!("Unimplemented class child {}", node.kind())
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
        let Some(statement_type) = node.named_child(0) else {
            eprintln!("Failed to get statement type from node");
            return None;
        };
        let Some(statement_definition) = statement_type.named_child(1) else {
            eprintln!("Failed to get statement definition from node");
            return None;
        };
        match statement_type.kind() {
            "method" => {
                eprintln!("HERE: Method");
                initial_build_method(statement_definition, MethodType::InstanceMethod, content)
            }
            "classmethod" => {
                initial_build_method(statement_definition, MethodType::ClassMethod, content)
            }
            _ => {
                eprintln!(
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
            let Some(keyword) = node.named_child(0) else {
                eprintln!("Failed to get keyword from node");
                continue;
            };
            if keyword.kind() == procedure_block {
                let Some(keyword_child) = node.child(0) else {
                    eprintln!("Failed to get keyword child from keyword");
                    continue;
                };
                if keyword_child.kind() == "keyword_not" {
                    self.is_procedure_block = Some(false);
                } else {
                    self.is_procedure_block = Some(true);
                }
            } else if keyword.kind() == language_keyword {
                if let Some(keyword_child) = node.child(1) {
                    if let Some(text) = content.get(keyword_child.byte_range()) {
                        if text.eq_ignore_ascii_case("tsql") {
                            self.default_language = Some(Language::TSql);
                        } else if text.eq_ignore_ascii_case("objectscript") {
                            self.default_language = Some(Language::Objectscript);
                        } else {
                            if let Some(s) = get_string_at_byte_range(content, keyword.byte_range())
                            {
                                eprintln!(
                                    "Language specified for keyword {:?} is not implemented",
                                    s
                                );
                                continue;
                            } else {
                                eprintln!("Failed to get text for keyword {:?}", text);
                            }
                        }
                    }
                }
            } else if keyword.kind() == inheritance_keyword {
                if let Some(keyword_child) = node.child(1) {
                    if let Some(text) = content.get(keyword_child.byte_range()) {
                        if text.eq_ignore_ascii_case("right") {
                            self.inheritance_direction = "right".to_string();
                        }
                    } else {
                        eprintln!("Couldn't get text for inheritance keyword");
                    }
                }
            }
        }
    }
}
