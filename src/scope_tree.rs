use crate::common::point_in_range;
use crate::scope_structures::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use tree_sitter::{Node, Point, Range};

impl Scope {
    fn new(start: Point, end: Point, parent: Option<ScopeId>, is_new_scope: bool) -> Self {
        Self {
            start,
            end,
            parent,
            children: Vec::new(),
            symbols: Vec::new(),
            defs: HashMap::new(), // all the symbols in this scope
            refs: HashMap::new(),
            is_new_scope,
        }
    }

    pub fn new_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        range: Range,
        scope: ScopeId,
    ) -> SymbolId {
        let id = SymbolId(self.defs.len());
        self.symbols.push(Symbol {
            name: name.clone(),
            kind,
            location: range,
            scope,
            references: Vec::new(),
        });
        self.defs.insert(name.clone(), id);
        id
    }
}

#[derive(Debug)]
pub struct ScopeTree {
    pub scopes: RwLock<HashMap<ScopeId, Scope>>,
    pub(crate) root: ScopeId,
    pub(crate) next_scope_id: usize,
}

impl Clone for ScopeTree {
    fn clone(&self) -> Self {
        let scopes_data = self.scopes.read().clone();

        Self {
            scopes: RwLock::new(scopes_data),
            root: self.root,
            next_scope_id: self.next_scope_id,
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
        }
    }

    pub fn add_scope(
        &mut self,
        start: Point,
        end: Point,
        parent: ScopeId,
        defs: Option<HashMap<String, SymbolId>>,
        is_new_scope: bool,
    ) -> ScopeId {
        let scope_id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;
        let scope = Scope {
            start,
            end,
            parent: Some(parent),
            children: Vec::new(),
            symbols: Vec::new(),
            defs: defs.unwrap_or(HashMap::new()),
            refs: HashMap::new(),
            is_new_scope,
        };

        // update parent to include this scope as a child
        if let Some(parent_scope) = self.scopes.write().get_mut(&parent) {
            parent_scope.children.push(scope_id);
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

    /// Adds the def to the scope
    fn add_variable(&self, node: Node, var_name: String, symbol_id: SymbolId) {
        let scope_id = self.find_current_scope(node.start_position()).unwrap();
        self.add_def(scope_id, var_name, symbol_id);
    }
}
