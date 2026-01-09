use crate::common::point_in_range;
use crate::scope_structures::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use tree_sitter::{Point, Range};

impl Scope {
    fn new(start: Point, end: Point, parent: Option<ScopeId>, is_new_scope: bool) -> Self {
        Self {
            start,
            end,
            parent,
            children: Vec::new(),
            variable_symbols: Vec::new(),
            method_symbols: Vec::new(),
            public_var_defs: HashMap::new(), // HashMap var name -> GlobalSymbol
            is_new_scope,
        }
    }

    fn new_method_symbol(
        &mut self,
        name: String,
        location: Range,
        scope_id: ScopeId,
    ) -> MethodSymbolId
    {
        let sym_id = MethodSymbolId(self.method_symbols.len());
        self.method_symbols.push( MethodSymbol {
            name,
            location,
            scope_id,
        });
        sym_id
    }

    fn new_variable_symbol(
        &mut self,
        name: String,
        location: Range,
        scope_id: ScopeId,
        var_dependencies: Vec<String>,
        property_dependencies: Vec<String>,
    ) -> VariableSymbolId {
        let sym_id = VariableSymbolId(self.variable_symbols.len());
        self.variable_symbols.push( VariableSymbol {
            name: name.clone(),
            location,
            scope_id,
            references: Vec::new(),
            var_dependencies,      // var names
            property_dependencies, // property names
        });
        sym_id
    }

    pub fn new_symbol_pub_variable(&mut self, name: String, id: VariableGlobalSymbolId) {
        self.public_var_defs.insert(name.clone(), id);
    }
}

#[derive(Debug)]
pub struct ScopeTree {
    pub scopes: RwLock<HashMap<ScopeId, Scope>>,
    pub(crate) root: ScopeId,
    pub(crate) next_scope_id: usize,
    private_variable_defs: HashMap<String, (ScopeId, VariableSymbolId)>,
    private_method_defs: HashMap<String, (ScopeId, MethodSymbolId)>,
    pub(crate) class_def: Option<ClassGlobalSymbolId>,
}

impl Clone for ScopeTree {
    fn clone(&self) -> Self {
        let scopes_data = self.scopes.read().clone();

        Self {
            scopes: RwLock::new(scopes_data),
            root: self.root,
            next_scope_id: self.next_scope_id,
            private_variable_defs: self.private_variable_defs.clone(),
            private_method_defs: self.private_method_defs.clone(),
            class_def: self.class_def,
        }
    }
}

impl ScopeTree {
    pub fn new() -> Self {
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
            private_variable_defs: HashMap::new(),
            private_method_defs: HashMap::new(),
            // public_method_defs: HashMap::new(),
            class_def: None,
        }
    }

    pub fn get_class_symbol(&self) -> Option<ClassGlobalSymbolId> {
        self.class_def
    }

    pub fn get_private_variable_symbol(&self, name: &str) -> Option<(ScopeId, VariableSymbolId)> {
        self.private_variable_defs.get(name).copied()
    }

    pub fn get_private_method_symbol(&self, name: &str) -> Option<(ScopeId, MethodSymbolId)> {
        self.private_method_defs.get(name).copied()
    }

    pub fn add_scope(
        &mut self,
        start: Point,
        end: Point,
        parent: ScopeId,
        is_new_scope: bool,
    ) -> ScopeId {
        let scope_id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;
        let scope = Scope {
            start,
            end,
            parent: Some(parent),
            children: Vec::new(),
            method_symbols: Vec::new(),
            variable_symbols: Vec::new(),
            public_var_defs: HashMap::new(),
            is_new_scope,
        };
        // update parent to include this scope as a child
        if let Some(parent_scope) = self.scopes.write().get_mut(&parent) {
            parent_scope.children.push(scope_id);
        }
        self.scopes.write().insert(scope_id, scope);
        scope_id
    }

    pub fn new_method_symbol(
        &mut self,
        name: String,
        range: Range,
    )
        -> MethodSymbolId
    {
        let scope_id = self.find_current_scope(range.start_point).unwrap();
        let mut scopes = self.scopes.write();
        let scope = scopes.get_mut(&scope_id).unwrap();
        let sym_id = scope.new_method_symbol(
            name.clone(),
            range,
            scope_id,
        );
        self.private_method_defs.insert(name, (scope_id, sym_id));
        sym_id
    }

    pub fn new_variable_symbol(
        &mut self,
        name: String,
        range: Range,
        var_deps: Vec<String>,
        prop_deps: Vec<String>,
    )
        -> VariableSymbolId
    {
        let scope_id = self.find_current_scope(range.start_point).unwrap();
        let mut scopes = self.scopes.write();
        let scope = scopes.get_mut(&scope_id).unwrap();
        let sym_id = scope.new_variable_symbol(
            name.clone(),
            range,
            scope_id,
            var_deps,
            prop_deps,
        );
        self.private_variable_defs.insert(name, (scope_id, sym_id));
        sym_id
    }

    pub fn new_public_var_symbol(
        &mut self,
        name: String,
        range: Range,
        symbol_id: VariableGlobalSymbolId,
    ) {
        let scope_id = self
            .find_current_scope(range.start_point)
            .expect("no scope found");
        let mut scopes = self.scopes.write();
        let scope = scopes.get_mut(&scope_id).expect("missing scope");
        scope.new_symbol_pub_variable(name.clone(), symbol_id);
    }

    pub fn find_current_scope(&self, pos: Point) -> Option<ScopeId> {
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
}
