use crate::common::point_in_range;
use crate::scope_structures::*;
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
            private_variable_defs: HashMap::new(),
            is_new_scope,
        }
    }

    fn new_method_symbol(
        &mut self,
        name: String,
        location: Range,
        scope_id: ScopeId,
    ) -> MethodSymbolId {
        let sym_id = MethodSymbolId(self.method_symbols.len());
        self.method_symbols.push(MethodSymbol {
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
        self.variable_symbols.push(VariableSymbol {
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
    pub scopes: HashMap<ScopeId, Scope>,
    pub(crate) root: ScopeId,
    pub(crate) next_scope_id: usize,
    private_method_defs: HashMap<String, (ScopeId, MethodSymbolId)>,
    pub(crate) class_def: ClassGlobalSymbolId,
}

impl Clone for ScopeTree {
    fn clone(&self) -> Self {
        Self {
            scopes: self.scopes.clone(),
            root: self.root,
            next_scope_id: self.next_scope_id,
            private_method_defs: self.private_method_defs.clone(),
            class_def: self.class_def,
        }
    }
}

impl ScopeTree {
    pub fn new(class_symbol_id: ClassGlobalSymbolId) -> Self {
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
        let mut scopes = HashMap::new();
        scopes.insert(root_id, root_scope);
        Self {
            scopes,
            root: root_id,
            next_scope_id: 1,
            private_method_defs: HashMap::new(),
            // public_method_defs: HashMap::new(),
            class_def: class_symbol_id,
        }
    }

    pub fn get_class_symbol(&self) -> ClassGlobalSymbolId {
        self.class_def
    }

    pub fn get_private_method_symbol_id(&self, name: &str) -> Option<(ScopeId, MethodSymbolId)> {
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
            private_variable_defs: HashMap::new(),
            is_new_scope,
        };
        // update parent to include this scope as a child
        if let Some(parent_scope) = self.scopes.get_mut(&parent) {
            parent_scope.children.push(scope_id);
        }
        self.scopes.insert(scope_id, scope);
        scope_id
    }

    pub fn new_method_symbol(&mut self, name: String, range: Range) -> Option<MethodSymbolId> {
        let Some(scope_id) = self.find_current_scope(range.start_point) else {
            eprintln!("Scope Id not found, tried to create new method symbol");
            return None;
        };
        let Some(scope) = self.scopes.get_mut(&scope_id) else {
            eprintln!("Scope not found, tried to create new method symbol");
            return None;
        };
        let sym_id = scope.new_method_symbol(name.clone(), range, scope_id);
        self.private_method_defs.insert(name, (scope_id, sym_id));
        Some(sym_id)
    }

    pub fn new_variable_symbol(
        &mut self,
        name: String,
        range: Range,
        var_deps: Vec<String>,
        prop_deps: Vec<String>,
    ) -> Option<VariableSymbolId> {
        let Some(scope_id) = self.find_current_scope(range.start_point) else {
            eprintln!("Scope Id not found, tried to create new variable symbol");
            return None;
        };
        let Some(scope) = self.scopes.get_mut(&scope_id) else {
            eprintln!("Scope not found, tried to create new variable symbol");
            return None;
        };
        let sym_id = scope.new_variable_symbol(name.clone(), range, scope_id, var_deps, prop_deps);
        Some(sym_id)
    }

    pub fn get_private_method_symbol(&self, name: String) -> Option<MethodSymbol> {
        let Some((scope_id, method_symbol_id)) = self.private_method_defs.get(&name).copied()
        else {
            eprintln!(
                "Scope Id Key DNE in Private Method Defs HashMap, tried to get method symbol id."
            );
            return None;
        };
        let Some(scope) = self.scopes.get(&scope_id) else {
            eprintln!("Tried to get scope in get_private_method_symbol, scope not found");
            return None;
        };

        if let Some(method_symbol) = scope.method_symbols.get(method_symbol_id.0) {
            Some(method_symbol.clone())
        } else {
            eprintln!("Private Method Symbol Not found");
            None
        }
    }

    pub fn new_public_var_symbol(
        &mut self,
        name: String,
        range: Range,
        symbol_id: VariableGlobalSymbolId,
    ) {
        let Some(scope_id) = self.find_current_scope(range.start_point) else {
            eprintln!("Scope Id Not found, tried to create new public variable symbol");
            return;
        };
        let Some(scope) = self.scopes.get_mut(&scope_id) else {
            eprintln!("Scope not found, tried to create public variable symbol");
            return;
        };
        scope.new_symbol_pub_variable(name.clone(), symbol_id);
    }

    pub fn find_current_scope(&self, pos: Point) -> Option<ScopeId> {
        let mut current = self.root;

        loop {
            let Some(scope) = self.scopes.get(&current) else {
                return None;
            };
            // iterate over children vector (which contains scopeid values)
            // searches for the first child that satisfies the condition of containing the point
            let child = scope.children.iter().find(|&&child_id| {
                let Some(child_scope) = self.scopes.get(&child_id) else {
                    return false;
                };
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
