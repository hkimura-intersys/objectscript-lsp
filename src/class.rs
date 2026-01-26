use crate::common::{generic_exit_statements, generic_skipping_statements, get_keyword, get_node_children, get_string_at_byte_range, start_of_function, successful_exit};
use crate::method::initial_build_method;
use crate::parse_structures::{
    Class, Language, Method, MethodType, PrivateMethodId, PublicMethodId,
};
use std::collections::HashMap;
use tree_sitter::{Node, Range};
/*
For Simplicity, I am not including the logic for parsing
include or include_gen files. This feature can be added later.
 */
impl Class {
    /// Creates a new `Class` with the given name and empty semantic state.
    ///
    /// Inheritance/imports/keywords/members are initialized to defaults; `active` is `true`.
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

    /// Resets this `Class` to a clean state and sets its `name` and `active` flag.
    ///
    /// Clears imports/inheritance/keywords/methods/properties/params/method_calls and restores
    /// default inheritance direction to `"left"`.
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

    /// Performs the first-pass parse of a class definition node into this `Class`.
    ///
    /// Extracts class keywords (ProcedureBlock, Language, InheritanceDirection) and collects
    /// method definitions from the class body. Does not compute imports, include files, or
    /// inherited/transitive semantics; those are handled later.
    ///
    /// Returns the parsed methods and their source ranges.
    pub fn initial_build(&mut self, node: Node, content: &str) -> Vec<(Method, Range)> {
        start_of_function("Class", "initial_build");
        let class_children = get_node_children(node);
        let mut methods = Vec::new();
        if class_children.len() < 2 {
            eprintln!(
                "initial_build: expected class_definition node, got kind={} named_children={}",
                node.kind(),
                class_children.len()
            );
            generic_exit_statements("Class", "initial_build");
            return Vec::new();
        }
        // skip keyword_class and class_name
        for node in class_children.iter().skip(2) {
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
                            eprintln!("Warning: Failed to get method from handle_class_statement_method");
                            generic_skipping_statements("Class: initial_build", "class body node", "node");
                            continue;
                        };

                        methods.push((method, method_range));
                    }
                }
                _ => {
                    eprintln!("Unimplemented class child {}", node.kind());
                    generic_skipping_statements("Class: initial_build", node.kind(), "node");
                    continue;
                }
            }
        }
        successful_exit("Class", "class initial_build");
        methods
    }

    /// Parses a `class_statement` node and returns the corresponding `Method` and its `Range`.
    ///
    /// Supports instance methods (`method`) and class methods (`classmethod`). Logs and returns
    /// `None` for unsupported statement kinds or malformed syntax nodes.
    fn handle_class_statement_method(
        &mut self,
        node: Node,
        content: &str,
    ) -> Option<(Method, Range)> {
        start_of_function("Class", "handle_class_statement_method");
        let Some(statement_type) = node.named_child(0) else {
            eprintln!("Failed to get statement type from node : {:?}", node);
            generic_exit_statements("Class", "handle_class_statement_method");
            return None;
        };
        let Some(statement_definition) = statement_type.named_child(1) else {
            eprintln!(
                "Failed to get statement definition from node {:?}",
                statement_type
            );
            generic_exit_statements("Class", "handle_class_statement_method");
            return None;
        };
        match statement_type.kind() {
            "method" => {
                successful_exit("Class", "handle_class_statement_method");
                initial_build_method(statement_definition, MethodType::InstanceMethod, content)
            }
            "classmethod" => {
                successful_exit("Class", "handle_class_statement_method");
                initial_build_method(statement_definition, MethodType::ClassMethod, content)
            }
            _ => {
                eprintln!(
                    "Unimplementated class statement {:?}",
                    statement_type.kind()
                );
                generic_exit_statements("Class", "handle_class_statement_method");
                None
            }
        }
    }

    /// Parses class-level keywords and updates `is_procedure_block`, `default_language`,
    /// and `inheritance_direction` accordingly.
    ///
    /// Currently recognizes ProcedureBlock, Language (tsql/objectscript), and Inheritance (right).
    /// Unrecognized or unsupported keyword values are logged and skipped.
    fn initial_build_class_keywords(&mut self, node: Node, content: &str) {
        start_of_function("Class", "initial_build_class_keywords");
        let class_keywords_children = get_node_children(node.clone());
        let procedure_block = get_keyword("class_keyword", "procedure");
        let language_keyword = get_keyword("class_keyword", "language");
        let inheritance_keyword = get_keyword("class_keyword", "inheritance");
        // each node here is a class_keyword
        for node in class_keywords_children.iter() {
            let Some(keyword) = node.named_child(0) else {
                eprintln!("Failed to get keyword from node");
                generic_skipping_statements("initial_build_class_keywords", "class keyword", "node");
                continue;
            };
            if keyword.kind() == procedure_block {
                let Some(keyword_child) = keyword.named_child(0) else {
                    eprintln!("Failed to get keyword child from keyword");
                    generic_skipping_statements("initial_build_class_keywords", "keyword child", "node");
                    continue;
                };
                if keyword_child.kind() == "keyword_not" {
                    self.is_procedure_block = Some(false);
                } else {
                    self.is_procedure_block = Some(true);
                }
            } else if keyword.kind() == language_keyword {
                if let Some(keyword_child) = keyword.named_child(1) {
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
                                generic_skipping_statements("initial_build_class_keywords", keyword.kind(), "node");
                                continue;
                            } else {
                                eprintln!("Failed to get text for keyword {:?}", text);
                                generic_skipping_statements("initial_build_class_keywords", keyword.kind(), "node");
                                continue;
                            }
                        }
                    }
                }
            } else if keyword.kind() == inheritance_keyword {
                if let Some(keyword_child) = keyword.named_child(1) {
                    if let Some(text) = content.get(keyword_child.byte_range()) {
                        if text.eq_ignore_ascii_case("right") {
                            self.inheritance_direction = "right".to_string();
                        }
                    } else {
                        eprintln!("Couldn't get text for inheritance keyword");
                        generic_skipping_statements("initial_build_class_keywords", keyword.kind(), "node");
                        continue;
                    }
                }
            }
        }
        successful_exit("Class", "initial_build_class_keywords");
    }

    /// Returns the `PublicMethodId` for `method_name`, if this class declares it as public.
    ///
    /// Logs and returns `None` if the method is not present in `public_methods`.
    pub fn get_public_method_id(&self, method_name: &str) -> Option<&PublicMethodId> {
        start_of_function("Class", "get_public_method_id");
        let result = self.public_methods.get(method_name);
        match result {
            None => {
                eprintln!("Potential Warning: There is no public method in this class with the name {:?}. The public methods in this class are {:?}", method_name, self.public_methods.keys());
                generic_exit_statements("Class", "get_public_method_id");
                result
            }

            Some(_) => {
                successful_exit("Class", "get_public_method_id");
                result
            },
        }
    }

    /// Returns the `PrivateMethodId` for `method_name`, if this class declares it as private.
    ///
    /// Logs and returns `None` if the method is not present in `private_methods`.
    pub fn get_private_method_id(&self, method_name: &str) -> Option<&PrivateMethodId> {
        start_of_function("Class", "get_private_method_id");
        let result = self.private_methods.get(method_name);
        match result {
            None => {
                eprintln!("Potential Warning: There is no private method in this class with the name {:?}. The private methods in this class are {:?}", method_name, self.private_methods.keys());
                generic_exit_statements("Class", "get_private_method_id");
                result
            }

            Some(_) => {
                successful_exit("Class", "get_private_method_id");
                result },
        }
    }
}
