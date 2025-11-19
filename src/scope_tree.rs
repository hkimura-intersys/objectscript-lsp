use parking_lot::RwLock;
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;
use tree_sitter::{Node, Point, Range};

#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(usize);

pub fn point_in_range(pos: Point, start: Point, end: Point) -> bool {
    if pos >= start && pos <= end {
        return true;
    };
    false
}

#[derive(Clone)]
struct Scope {
    pub(crate) start: Point, // have to convert to Position for ls client
    pub(crate) end: Point,
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    pub(crate) defs: HashMap<String, Range>, // only will store the original def, not redefs
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

    fn add_def(&mut self, def_name:String, def_range: Range, overwrite: bool) {
        if self.defs.contains_key(&def_name) {
            if overwrite {
                self.defs.insert(def_name.clone(), def_range.clone());
            }
        }
        else {
            self.defs.insert(def_name.clone(), def_range.clone());
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

    pub fn add_scope(&mut self, start: Point, end: Point, parent: ScopeId, defs: Option<HashMap<String,Range>>, is_new_scope:bool) -> ScopeId {
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

    fn find_declaration(&self, identifier: &str, scope_id: ScopeId) -> Option<Range> {
        let mut current = Some(scope_id);

        while let Some(id) = current {
            let scopes = self.scopes.read();
            let scope = scopes.get(&scope_id).unwrap();

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

    // should start on the method_def level
    fn cls_build_symbol_table(&self, node: Node, not_procedure:bool) {
        match node.kind() {
            "method_definition" => {
                let method_name_node = node.child(0).unwrap();
                let method_name = self.source_content[method_name_node.byte_range()].to_string();
                let mut is_pub = true;
                let mut not_procedure = false;
                let mut cursor = node.walk();
                let method_children = node.children(& mut cursor).collect::<Vec<_>>();
                for child in method_children[1..].iter() {
                    match child.kind() {
                        "arguments" => {
                            // add these to the method scope
                            let mut cursor = child.walk();
                            let param_children = child.children(& mut cursor).collect::<Vec<_>>();
                            // this would be argument, not identifier yet
                            // not sure if we want to store if by ref or output were passed in, but for now I won't be
                            for child in param_children {
                                match child.kind() {
                                    "argument" => {
                                        let mut cursor = child.walk();
                                        let argument_children = child.children(& mut cursor).collect::<Vec<_>>();
                                        for child in argument_children {
                                            match child.kind() {
                                                "identifier" => {
                                                    let name = self.source_content[child.byte_range()].to_string();
                                                    let curr_scope_id = self.find_current_scope(child.start_position()).unwrap();
                                                    let mut scopes = self.scopes.write();
                                                    let defs = &mut scopes.get_mut(&curr_scope_id).unwrap().defs;
                                                    if defs.contains_key(&name) {
                                                        // The only scenario I can think of where this happens is if someone tried to
                                                        // name a variable the same name, which should be syntax err
                                                        // TODO: make sure this works for public local vars too.
                                                        // TODO: do public local vars get passed in ? Or they don't have to?
                                                        panic!("Variable {} already exists in method scope", name);

                                                    }
                                                    else {
                                                        defs.insert(name.clone(),child.range());
                                                    }
                                                },
                                                _ => continue,
                                            }
                                        }
                                    },
                                    _ => {panic!("Arguments child should be an argument, not {}", child.kind());}
                                }
                            }
                        },
                        "method_keywords" => {
                            let mut cursor = child.walk();
                            let keyword_children = child.children(& mut cursor).collect::<Vec<_>>();
                            for child in keyword_children {
                                match child.kind() {
                                    "kw_NotProcedureBlock" => {
                                        not_procedure = true;
                                    },
                                    "kw_Private" => {
                                        is_pub = false;
                                    },
                                    "kw_PublicList" => {
                                        let mut cursor = child.walk();
                                        let public_variables = child.children(& mut cursor).collect::<Vec<_>>();
                                        let curr_scope_id = self.find_current_scope(child.start_position()).unwrap();
                                        let mut scopes = self.scopes.write();
                                        let curr_scope = scopes.get_mut(&curr_scope_id).unwrap();
                                        for var in public_variables {
                                            let var_name = self.source_content[var.byte_range()].to_string();
                                            curr_scope.public_variables_in_scope.push(var_name.clone());
                                            curr_scope.defs.insert(var_name.clone(), var.range());
                                        }
                                        drop(scopes);
                                    }
                                    _ => {continue;}
                                }
                            }
                        },
                        _ => {continue;}
                    }
                }
                if is_pub {
                    // let parent_scope = scopes.get_mut(&curr_scope.parent.unwrap()).unwrap();
                    // let mut defs = parent_scope.defs.clone();
                    // defs.insert(method_name.clone(), method_name_node.range());
                    // drop(scopes);
                    // TODO: this needs to be added to global static hashmap.
                    // TODO: I think I want to start defining things in code, this
                    // TODO: will make code completion much easier.
                }
                else {
                    // private method, only available to things in the class.
                    let curr_scope_id = self.find_current_scope(node.start_position()).unwrap();
                    let mut scopes = self.scopes.write();
                    let curr_scope = scopes.get_mut(&curr_scope_id).unwrap();
                    let parent_scope = scopes.get_mut(&curr_scope.parent.unwrap()).unwrap();
                    let mut defs = parent_scope.defs.clone();
                    defs.insert(method_name.clone(), method_name_node.range());
                    drop(scopes);
                }
                let method_body = node.child_by_field_name("body").unwrap();
                self.cls_build_symbol_table(method_body, not_procedure);
                let mut cursor = method_body.walk();
                let method_body_children = method_body.children(& mut cursor).collect::<Vec<_>>();
                for child in method_body_children {
                    self.cls_build_symbol_table(child, not_procedure);
                }
            },
            "statement" => {
                // always has one child
                let command_node = node.child(0).unwrap();
                self.build_symbol_table(command_node); // setting to true, but doesn't actually matter
            },
            "command_set" => {
                // first child is keyword_set, we want the second child
                let set_argument = node.child(1).unwrap();
                let set_def = set_argument.child(0).unwrap(); // lhs
                let set_def_name = self.source_content[set_def.byte_range()].to_string();
                if !not_procedure {
                    let mut scopes = self.scopes.write();
                    let scope_id = self.find_current_scope(set_def.start_position()).unwrap();
                    let mut scope = scopes.get_mut(&scope_id).unwrap();
                    scope.add_def(set_def_name.clone(),set_argument.range(), false);
                    drop(scopes); // drop lock guard
                }
                let set_rhs = set_argument.child(1).unwrap();
                self.cls_build_symbol_table(set_rhs,not_procedure);
            },
            "expression" => {
                // TODO: need to add expr_tail and expr atom here
                // TODO: need to verify this only ever leads to refs
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                for child in children {
                    self.cls_build_symbol_table(child, not_procedure);
                }
            },
            "expr_atom" => {

            }
            "command_write" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                let mut refs: Vec<Option<Node>> = Vec::new();

            },
            "command_for" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect(); // skip keyword for and for parameter
                for child in children {
                    self.cls_build_symbol_table(child,not_procedure);
                }
            },
            "for_parameter" => {
                let for_parameter_def = node.child(0).unwrap();
                let for_parameter_name = self.source_content[for_parameter_def.byte_range()].to_string();
                let curr_scope_id = self.find_current_scope(for_parameter_def.start_position()).unwrap();
                let mut scopes = self.scopes.write();
                let mut curr_scope = scopes.get(&curr_scope_id).unwrap();
                curr_scope.add_def(for_parameter_name.clone(), node.range(), false);
                drop(scopes);
            },

            _ => {}
        }
    }
}
