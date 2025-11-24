use parking_lot::RwLock;
use std::collections::HashMap;
use crate::parse_structures::*;
use crate::semantic::*;
use tree_sitter::{Node, Point, Range};
use tower_lsp::lsp_types::Url;

#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(usize);

pub fn point_in_range(pos: Point, start: Point, end: Point) -> bool {
    if pos >= start && pos <= end {
        return true;
    };
    false
}
#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) start: Point, // have to convert to Position for ls client
    pub(crate) end: Point,
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    pub(crate) defs: HashMap<String, SymbolId>, // only will store the original def, not redefs
    refs: HashMap<String, Vec<Range>>,
    is_new_scope: bool, // this is for legacy code only new a,b should give a syntax error for cls files
    public_variables_in_scope: Vec<String>, // for procedure blocks, whatever is declared in PublicList
}

impl Scope {
    fn new(start: Point, end: Point, parent: Option<ScopeId>, is_new_scope: bool) -> Self {
        Self {
            start,
            end,
            parent,
            children: Vec::new(),
            defs: HashMap::new(),
            refs: HashMap::new(),
            is_new_scope,
            public_variables_in_scope: Vec::new(),
        }
    }
    fn add_child(&mut self, child: ScopeId) {
        self.children.push(child);
    }

    fn add_def(&mut self, def_name:String, sym_id: SymbolId) {
        if !self.defs.contains_key(&def_name) {
            self.defs.insert(def_name.clone(), sym_id.clone());
        }
    }
}

pub(crate) struct ScopeTree {
    pub(crate) scopes: RwLock<HashMap<ScopeId, Scope>>,
    pub(crate) root: ScopeId,
    next_scope_id: usize,
    source_content: String, // store the source content to be able to build the scope
}

impl Clone for ScopeTree {
    fn clone(&self) -> Self {
        let scopes_data = self.scopes.read().clone();

        Self {
            scopes: RwLock::new(scopes_data),
            root: self.root,
            next_scope_id: self.next_scope_id,
            source_content: self.source_content.clone(),
        }
    }
}

impl ScopeTree {
    pub fn new(source_content: String) -> Self {
        let root_id = ScopeId(0);
        let root_scope = Scope::new(
            Point { row: 0, column: 0 },
            Point {
                row: usize::MAX,
                column: usize::MAX,
            },
            None,
            false,
        );
        let scopes = RwLock::new(HashMap::new());
        scopes.write().insert(root_id, root_scope);
        Self {
            scopes,
            root: root_id,
            next_scope_id: 1,
            source_content,
        }
    }

    pub fn add_scope(&mut self, start: Point, end: Point, parent: ScopeId, defs: Option<HashMap<String,SymbolId>>, is_new_scope:bool) -> ScopeId {
        let scope_id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;
        let scope = Scope {
            start,
            end,
            parent: Some(parent),
            children: Vec::new(),
            defs: defs.unwrap_or(HashMap::new()),
            refs: HashMap::new(),
            is_new_scope,
            public_variables_in_scope: Vec::new(),
        };

        // update parent to include this scope as a child
        if let Some(parent_scope) = self.scopes.write().get_mut(&parent) {
            parent_scope.add_child(scope_id);
        }

        self.scopes.write().insert(scope_id, scope);
        scope_id
    }

    /// This function will be called by the goto_definition function.
    fn find_declaration(&self, identifier: &str, scope_id: ScopeId) -> Option<SymbolId> {
        let mut current = Some(scope_id);

        while let Some(id) = current {
            let scopes = self.scopes.read();
            let scope = scopes.get(&id).unwrap();

            if let Some(def) = scope.defs.get(identifier) {
                return Some(def.clone());
            }

            current = scope.parent;
        }
        None
    }
    fn find_current_scope(&self, pos: Point) -> Option<ScopeId> {
        let mut current = self.root;

        loop {
            let scopes = self.scopes.read();
            let scope = scopes.get(&current).unwrap();
            // iterate over children vector (which contains scopeid values)
            // searches for the first child that satisfies the condition of containing the point
            let child = scope.children.iter().find(|&&child_id| {
                let child_scope = scopes.get(&child_id).unwrap();
                point_in_range(pos, child_scope.start, child_scope.end)
            });
            match child {
                Some(&child_id) => current = child_id,
                None => {
                    return Some(current);
                }
            }
        }
    }

    pub(crate) fn get_new_command_args(&self, node: Node) -> Vec<String> {
        let mut args = Vec::new();
        if node.kind() != "command_new" {
            panic!("{:?} is not a command new", node.kind());
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        if children.len() > 1 { // has arguments
            for child in children[1..].iter() {
                let child_name = self.source_content[child.byte_range()].to_string();
                args.push(child_name);
            }
        }
        args
    }

    fn get_node_children(&self, node: Node) -> Vec<Node> {
        let mut cursor = node.walk();
        node.children(&mut cursor).collect::<Vec<Node>>()
    }

    /// create the class Struct that will be used in the local_semantic model
    /// Global Semantic Model should be created in workspace.rs, which should call this func
    fn create_class_and_start_build(&self, node: Node, global_semantic: &mut GlobalSemanticModel, url:Url) {
        let range = node.range();
        let class_name = self.get_class_name(node);
        let current_scope = self.find_current_scope(node.start_position()).unwrap();

        let class = Class::new(class_name, range, current_scope);
        let mut local_semantic = LocalSemanticModel::new(class);
        self.cls_build_symbol_table(node, &mut local_semantic, global_semantic, url);
    }

    fn add_def(&self, scope_id: ScopeId, name: String, symbol_id: SymbolId) {
        let mut scopes = self.scopes.write();
        let defs = &mut scopes.get_mut(&scope_id).unwrap().defs;
        if defs.contains_key(&name) {
            panic!("{:?} is already defined in this scope", name);
        } else {
            defs.insert(name.clone(), symbol_id);
        }
        drop(scopes);
    }

    /// given a variable node, add the variable to the local semantic table, and create a symbol for it
    /// additionally adds the global ref if the variable is public
    fn add_variable(&self, node:Node, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel, method_name:String, is_public: bool, url:Url) {
        let var_name = self.source_content[node.byte_range()].to_string();
        let scope_id = self.find_current_scope(node.start_position()).unwrap();
        let range = node.range();
        let variable = Var::Variable(Variable {
            name: var_name.clone(),
            var_type: None,
            range,
            is_public,
        });
        if is_public {
            // add var to local_semantic, which also adds var_id to the method.public_vars hashmap
            let var_id = local_semantic.new_public_var(method_name.clone(), variable);
            // add to global_semantic
            global_semantic.new_public_local_var(url.clone(),var_id,var_name.clone());
            // add symbol to local_semantic
            let symbol_id = local_semantic.new_symbol(var_name.clone(),SymbolKind::PubVar(var_id), range, scope_id);
            self.add_def(scope_id,var_name,symbol_id);
        }

        else {
            // add var to local_semantic, which also adds var_id to the method.priv_vars hashmap
            let var_id = local_semantic.new_private_var(method_name.clone(), variable);
            // add symbol to local_semantic
            let symbol_id = local_semantic.new_symbol(var_name.clone(),SymbolKind::PrivVar(var_id), range, scope_id);
            self.add_def(scope_id,var_name,symbol_id);
        }

    }

    fn handle_class_keywords(&self, node: Node, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel) {
        let class_keyword_children = self.get_node_children(node.clone());
        for node in class_keyword_children[1..].iter() {
            match node.kind() {
                "kw_NotProcedureBlock" => {
                    local_semantic.class.is_procedure_block = false;
                },
                _ => { //TODO
                    println!("TODO")
                }
            }
        }
    }

    fn handle_class_property(&self, node: Node, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel) {
        // vars used in ClassProperty Struct
        let mut is_public = true;
        let range = node.range();
        let starting_pos = node.start_position();
        let property_name = self.source_content[node.child(1).unwrap().byte_range()].to_string();
        let mut property_type = None;

        let property_children = self.get_node_children(node.clone());

        for node in property_children[2..].iter() {
            match node.kind() {
                "property_type" => {
                    property_type = Some(self.source_content[node.child(1).unwrap().byte_range()].to_string());
                },
                "property_keywords" => {
                    let property_keyword_children = self.get_node_children(node.clone());
                    for keyword in property_keyword_children {
                        match keyword.kind() {
                            "kw_Private" => {
                                is_public = false;
                            }
                            _ => { println!("TODO: STILL NEED TO IMPLEMENT THE REST OF THE KEYWORDS") }
                        }
                    }
                },
                _ => { panic!("Unknown node type for property child nodes: {}", node.kind()); }
            }
        }
        // create property
        let class_property = ClassProperty {
            name: property_name.clone(),
            property_type,
            is_public,
            range
        };

        // add class property struct to local semantic, which also adds the PropertyId to the class.properties hashmap.
        let property_id = local_semantic.new_class_property(class_property);

        // add class property Id to scope
        let curr_scope_id = self.find_current_scope(starting_pos).unwrap();
        let symbol_id = local_semantic.new_symbol(property_name.clone(), SymbolKind::ClassProperty(property_id), range, curr_scope_id);
        self.add_def(curr_scope_id, property_name.clone(), symbol_id);
    }

    fn handle_class_parameter(&self, node: Node, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel) {
        let param_name = self.source_content[node.child(1).unwrap().byte_range()].to_string();
        let param_range = node.range();
        let starting_position = node.start_position();
        let mut property_type = None;
        let mut default_argument_value = None;
        let parameter_children = self.get_node_children(node.clone());

        for node in parameter_children[2..].iter() {
            match node.kind() {
                "default_argument_value" => {
                    default_argument_value = Some(self.source_content[node.byte_range()].to_string());
                },
                "property_type" => {
                    property_type = Some(self.source_content[node.child(1).unwrap().byte_range()].to_string());
                },
                "parameter_keywords" => {
                    println!("TODO: STILL NEED TO IMPLEMENT")
                }
                _ => { panic!("Unknown node type for parameter child nodes: {}", node.kind()); }
            }
        }

        // create class parameter, and add it to local semantic
        let class_parameter = ClassParameter {
            name: param_name.clone(),
            property_type,
            default_argument_value,
            range: param_range,
        };

        let parameter_id = local_semantic.new_class_parameter(class_parameter);

        let curr_scope_id = self.find_current_scope(starting_position).unwrap();
        let symbol_id = local_semantic.new_symbol(param_name.clone(), SymbolKind::ClassParameter(parameter_id), param_range, curr_scope_id);
        self.add_def(curr_scope_id, param_name.clone(), symbol_id);
    }

    fn handle_method_return_type(&self, node: Node, method_id:MethodId, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel) {}



    fn handle_method_arguments(&self, node: Node, method_id:MethodId, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel, url:Url) {
        let method_arguments_children = self.get_node_children(node.clone());
        let method = local_semantic.methods.get_mut(method_id.0).unwrap();
        // this would be argument, not identifier yet
        // not sure if we want to store if by ref or output were passed in, but for now I won't be
        for node in method_arguments_children {
            match node.kind() {
                "argument" => {
                    let argument_children = self.get_node_children(node.clone());
                    let argument_name = self.source_content[node.child(0).unwrap().byte_range()].to_string();
                    for node in argument_children {
                        match node.kind() {
                            "identifier" => {
                                self.add_variable(node, local_semantic, global_semantic, method.name.clone(),false,url.clone());
                            },
                            "typename" => {
                                let var_id = method.priv_vars.get(&argument_name).unwrap();
                                let variable = local_semantic.vars.get_mut(var_id.0).unwrap();
                                let type_name = self.source_content[node.byte_range()].to_string();
                                let data_type:Option<DataType> = match type_name.as_str() {
                                    "%String" => Some(DataType::String),
                                    "%Decimal" => Some(DataType::Decimal),
                                    "%Double" => Some(DataType::Double),
                                    "%BigInt" => Some(DataType::BigInt),
                                    "%Integer" => Some(DataType::Integer),
                                    "%Boolean" => Some(DataType::Boolean),
                                    "%DateTime" => Some(DataType::DateTime),
                                    "%Date" => Some(DataType::Date),
                                    "%Time" => Some(DataType::Time),
                                    "%Counter" => Some(DataType::Counter),
                                    "%SmallInt" => Some(DataType::SmallInt),
                                    "%TinyInt" => Some(DataType::TinyInt),
                                    "%Binary" => Some(DataType::Binary),
                                    "%Char" => Some(DataType::Char),
                                    "%EnumString" => Some(DataType::EnumString),
                                    "%ExactString" => Some(DataType::ExactString),
                                    "%List" => Some(DataType::List),
                                    "%ListOfBinary" => Some(DataType::ListOfBinary),
                                    "%Status" => Some(DataType::Status),
                                    "%Name" => Some(DataType::Name),
                                    _ => None
                                };
                                match variable {
                                    Var::Variable(variable) => {
                                        variable.var_type = data_type;
                                    }
                                    _ => {println!("Should be variable for method parameters, but instead was {}", variable)}
                                }
                            }
                            _ => println!("TODO: Still need to implement logic for method argument child node: {}",node.kind())
                        }
                    }
                },
                _ => {panic!("TODO: Still need to implement logic for method argument node: {}",node.kind());}
            }
        }
    }


    fn handle_method_keywords(&self, node: Node, method_id:MethodId, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel, url: Url) {
        let method = local_semantic.methods.get_mut(method_id.0).unwrap();
        let mut is_procedure_block_declared = false;
        let keyword_children = self.get_node_children(node.clone());
        for node in keyword_children {
            match node.kind() {
                "kw_NotProcedureBlock" => {
                    method.is_procedure_block = false;
                },
                "kw_ProcedureBlock" => {
                    is_procedure_block_declared = true;
                }
                "kw_Private" => {
                    method.is_public = false;
                },
                "kw_PublicList" => {
                    let public_variables = self.get_node_children(node.clone());
                    for var in public_variables {
                        self.add_variable(var, local_semantic, global_semantic, method.name.clone(),true,url.clone());
                    }
                }
                _ => {println!("Unimplemented node type for method keywords: {}", node.kind());}
            }
        }

        if !is_procedure_block_declared { // default based on class settings
            let is_procedure_block = local_semantic.class.is_procedure_block;
            method.is_procedure_block = is_procedure_block;
        }
    }

    fn handle_method_body(&self, node: Node, method_id:MethodId, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel) {

    }

    // should start on the class level
    fn cls_build_symbol_table(&self, node: Node, local_semantic: &mut LocalSemanticModel, global_semantic: &mut GlobalSemanticModel, url: Url) {
        match node.kind() {
            "class_definition" => {
                // get all important variables
                let class_children = self.get_node_children(node);
                for node in class_children[2..].iter() {
                    match node.kind() {
                        "class_keywords" => {
                            self.handle_class_keywords(node.clone(), local_semantic, global_semantic);
                        },
                        "class_extends" => {
                            let mut inherited_class_node = node.child(1).unwrap();
                            if inherited_class_node.kind() != "identifier" {
                                panic!("Inherited class name should be an identifier, but instead was {}", inherited_class_node.kind());
                            }
                            local_semantic.class.inherited_class = Some(self.source_content[inherited_class_node.byte_range()].to_string());
                        },
                        "class_body" => {
                            let class_statements = self.get_node_children(node.clone());
                            for class_statement in class_statements {
                                let node = class_statement.child(0).unwrap(); // the node within the class statement wrapper
                                match node.kind() {
                                    "classmethod" | "method" => {
                                        /*
                                        pub struct ClassMethod {
                                        pub return_type: Option<DataType>,
                                        pub name: String,
                                        pub range: Range,
                                        pub pub_vars : HashMap<String,VarId>,
                                        pub priv_vars: HashMap<String,VarId>,
                                        pub block_scope: ScopeId,
                                        pub is_public: bool,
                                        pub is_procedure_block: bool,
                                        }
                                         */
                                        let range = node.range();
                                        let scope = self.find_current_scope(node.start_position()).unwrap();
                                        let mut method_type = MethodType::InstanceMethod;
                                        if node.kind() == "classmethod" {
                                            method_type = MethodType::ClassMethod;
                                        }
                                        let method_definition = node.child(1).unwrap();
                                        let method_definition_children = self.get_node_children(method_definition); // method definition
                                        let method_name = self.source_content[method_definition_children[0].byte_range()].to_string();

                                        // create actual method struct
                                        let method = Method::new(method_name.clone(),range,method_type,scope);

                                        // add method to local_semantic
                                        let method_id = local_semantic.new_method(method);

                                        // create symbol for method
                                        let symbol_id = local_semantic.new_symbol(method_name.clone(),SymbolKind::Method(method_id),range,scope);
                                        let scope_id = self.find_current_scope(node.start_position()).unwrap();

                                        // add symbol to defs
                                        self.add_def(scope_id,method_name,symbol_id);

                                        for node in method_definition_children[1..].iter() {
                                            match node.kind() {
                                                "arguments" => {
                                                    // these are private variables in method's scope
                                                    self.handle_method_arguments(node.clone(), method_id, local_semantic, global_semantic, url.clone());
                                                },
                                                "return_type" => {
                                                    self.handle_method_return_type(node.clone(), method_id, local_semantic, global_semantic);

                                                },
                                                "method_keywords" => { // check for procedure block and private
                                                    self.handle_method_keywords(node.clone(), method_id, local_semantic, global_semantic, url.clone());

                                                },
                                                "core_method_body_content" | "expression_method_body_content" | "external_method_body_content" => {
                                                    // this is the method body
                                                }
                                                _ => { println!("Unimplemented method definition child :{}", node.kind()); }
                                            }
                                        }
                                    },
                                    "property" => {
                                      self.handle_class_property(node.clone(), local_semantic, global_semantic);
                                    },

                                    "parameter" => {
                                        self.handle_class_parameter(node.clone(), local_semantic, global_semantic);
                                    },

                                    _ => {
                                        println!("TODO: STILL HAVEN'T ADDED $.relationship, $.foreignkey,$.query,$.index, $.trigger,$.xdata,$.projection,$.storage")
                                    }
                                }
                            }
                        },
                        _ => { println!("Unimplemented class child {}", node.kind()) }
                    }
                }
            },
        }
    }
    //     "method_definition" => {
    //             let method_name_node = node.child(0).unwrap();
    //             let method_name = self.source_content[method_name_node.byte_range()].to_string();
    //             let mut is_pub = true;
    //             let mut not_procedure = false;
    //             let mut cursor = node.walk();
    //             let method_children = node.children(& mut cursor).collect::<Vec<_>>();
    //             for child in method_children[2..].iter() {
    //                 match child.kind() {
    //                     "arguments" => {
    //                         // add these to the method scope
    //                         let mut cursor = child.walk();
    //                         let param_children = child.children(& mut cursor).collect::<Vec<_>>();
    //                         // this would be argument, not identifier yet
    //                         // not sure if we want to store if by ref or output were passed in, but for now I won't be
    //                         for child in param_children {
    //                             match child.kind() {
    //                                 "argument" => {
    //                                     let mut cursor = child.walk();
    //                                     let argument_children = child.children(& mut cursor).collect::<Vec<_>>();
    //                                     for child in argument_children {
    //                                         match child.kind() {
    //                                             "identifier" => {
    //                                                 let name = self.source_content[child.byte_range()].to_string();
    //                                                 let curr_scope_id = self.find_current_scope(child.start_position()).unwrap();
    //                                                 let mut scopes = self.scopes.write();
    //                                                 let defs = &mut scopes.get_mut(&curr_scope_id).unwrap().defs;
    //                                                 if defs.contains_key(&name) {
    //                                                     // The only scenario I can think of where this happens is if someone tried to
    //                                                     // name a variable the same name, which should be syntax err
    //                                                     // TODO: make sure this works for public local vars too.
    //                                                     // TODO: do public local vars get passed in ? Or they don't have to?
    //                                                     panic!("Variable {} already exists in method scope", name);
    //
    //                                                 }
    //                                                 else {
    //
    //                                                     defs.insert(name.clone(),child.range());
    //                                                 }
    //                                             },
    //                                             _ => continue,
    //                                         }
    //                                     }
    //                                 },
    //                                 _ => {panic!("Arguments child should be an argument, not {}", child.kind());}
    //                             }
    //                         }
    //                     },
    //                     "method_keywords" => {
    //                         let mut cursor = child.walk();
    //                         let keyword_children = child.children(& mut cursor).collect::<Vec<_>>();
    //                         for child in keyword_children {
    //                             match child.kind() {
    //                                 "kw_NotProcedureBlock" => {
    //                                     not_procedure = true;
    //                                 },
    //                                 "kw_Private" => {
    //                                     is_pub = false;
    //                                 },
    //                                 "kw_PublicList" => {
    //                                     let mut cursor = child.walk();
    //                                     let public_variables = child.children(& mut cursor).collect::<Vec<_>>();
    //                                     let curr_scope_id = self.find_current_scope(child.start_position()).unwrap();
    //                                     let mut scopes = self.scopes.write();
    //                                     let curr_scope = scopes.get_mut(&curr_scope_id).unwrap();
    //                                     for var in public_variables {
    //                                         let var_name = self.source_content[var.byte_range()].to_string();
    //                                         curr_scope.public_variables_in_scope.push(var_name.clone());
    //                                         curr_scope.defs.insert(var_name.clone(), var.range());
    //                                     }
    //                                     drop(scopes);
    //                                 }
    //                                 _ => {continue;}
    //                             }
    //                         }
    //                     },
    //                     _ => {continue;}
    //                 }
    //             }
    //             if is_pub {
    //                 // let parent_scope = scopes.get_mut(&curr_scope.parent.unwrap()).unwrap();
    //                 // let mut defs = parent_scope.defs.clone();
    //                 // defs.insert(method_name.clone(), method_name_node.range());
    //                 // drop(scopes);
    //                 // TODO: this needs to be added to global static hashmap.
    //                 // TODO: I think I want to start defining things in code, this
    //                 // TODO: will make code completion much easier.
    //             }
    //             else {
    //                 // private method, only available to things in the class.
    //                 let curr_scope_id = self.find_current_scope(node.start_position()).unwrap();
    //                 let mut scopes = self.scopes.write();
    //                 let curr_scope = scopes.get_mut(&curr_scope_id).unwrap();
    //                 let parent_scope = scopes.get_mut(&curr_scope.parent.unwrap()).unwrap();
    //                 let mut defs = parent_scope.defs.clone();
    //                 defs.insert(method_name.clone(), method_name_node.range());
    //                 drop(scopes);
    //             }
    //             let method_body = node.child_by_field_name("body").unwrap();
    //             self.cls_build_symbol_table(method_body, not_procedure);
    //             let mut cursor = method_body.walk();
    //             let method_body_children = method_body.children(& mut cursor).collect::<Vec<_>>();
    //             for child in method_body_children {
    //                 self.cls_build_symbol_table(child, not_procedure);
    //             }
    //         },
    //         "statement" => {
    //             // always has one child
    //             let command_node = node.child(0).unwrap();
    //             self.build_symbol_table(command_node); // setting to true, but doesn't actually matter
    //         },
    //         "command_set" => {
    //             // first child is keyword_set, we want the second child
    //             let set_argument = node.child(1).unwrap();
    //             let set_def = set_argument.child(0).unwrap(); // lhs
    //             let set_def_name = self.source_content[set_def.byte_range()].to_string();
    //             if !not_procedure {
    //                 let mut scopes = self.scopes.write();
    //                 let scope_id = self.find_current_scope(set_def.start_position()).unwrap();
    //                 let mut scope = scopes.get_mut(&scope_id).unwrap();
    //                 scope.add_def(set_def_name.clone(),set_argument.range());
    //                 drop(scopes); // drop lock guard
    //             }
    //             let set_rhs = set_argument.child(1).unwrap();
    //             self.cls_build_symbol_table(set_rhs,not_procedure);
    //         },
    //         "expression" => {
    //             // TODO: need to add expr_tail and expr atom here
    //             // TODO: need to verify this only ever leads to refs
    //             let mut cursor = node.walk();
    //             let children: Vec<_> = node.children(&mut cursor).collect();
    //             for child in children {
    //                 self.cls_build_symbol_table(child, not_procedure);
    //             }
    //         },
    //         "expr_atom" => {
    //
    //         }
    //         "command_write" => {
    //             let mut cursor = node.walk();
    //             let children: Vec<_> = node.children(&mut cursor).collect();
    //             let mut refs: Vec<Option<Node>> = Vec::new();
    //
    //         },
    //         "command_for" => {
    //             let mut cursor = node.walk();
    //             let children: Vec<_> = node.children(&mut cursor).collect(); // skip keyword for and for parameter
    //             for child in children {
    //                 self.cls_build_symbol_table(child,not_procedure);
    //             }
    //         },
    //         "for_parameter" => {
    //             let for_parameter_def = node.child(0).unwrap();
    //             let for_parameter_name = self.source_content[for_parameter_def.byte_range()].to_string();
    //             let curr_scope_id = self.find_current_scope(for_parameter_def.start_position()).unwrap();
    //             let mut scopes = self.scopes.write();
    //             let mut curr_scope = scopes.get(&curr_scope_id).unwrap();
    //             curr_scope.add_def(for_parameter_name.clone(), node.range());
    //             drop(scopes);
    //         },
    //
    //         _ => {}
    //     }
    // }

    fn get_class_name(&self, node: Node) -> String {
        let class_name_node = node.child(1).unwrap();
        let class_name = self.source_content[class_name_node.byte_range()].to_string();
        class_name
    }




}
