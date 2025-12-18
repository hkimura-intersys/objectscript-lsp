use parking_lot::RwLock;
use std::collections::HashMap;
use serde_json::Value;
use crate::parse_structures::*;
use crate::semantic::*;
use tree_sitter::{Node, Point, Range};
use tower_lsp::lsp_types::Url;

// TODO: I want to think more about if it is possible to NOT store the content here as well
#[derive(Copy, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ScopeId(usize);

pub fn point_in_range(pos: Point, start: Point, end: Point) -> bool {
    if pos >= start && pos <= end {
        return true;
    };
    false
}

/// helper function to recursively walk tree sitter parsed tree
pub fn walk_tree<F>(node: Node, callback: &mut F)
where
    F: FnMut(Node),
{
    callback(node);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(child, callback);
    }
}

/// helper function to check if the given node creates a new scope
/// TODO: do dotted statements
pub fn cls_is_scope_node(node: Node) -> bool {
    if node.kind() == "classmethod" || node.kind() == "method" {
        return true;
    }
    false
}


#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub(crate) start: Point, // have to convert to Position for ls client
    pub(crate) end: Point,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) defs: HashMap<String, SymbolId>, // only will store the original def, not redefs
    pub(crate) refs: HashMap<String, Vec<Range>>,
    pub(crate) is_new_scope: bool, // this is for legacy code only new a,b should give a syntax error for cls files
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
        }
    }
}

#[derive(Debug)]
pub struct ScopeTree {
    pub scopes: RwLock<HashMap<ScopeId, Scope>>,
    pub(crate) root: ScopeId,
    pub(crate) next_scope_id: usize,
    pub(crate) source_content: String, // store the source content to be able to build the scope
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

    pub fn add_scope(&mut self, start: Point, end: Point, parent: ScopeId, defs: Option<HashMap<String, SymbolId>>, is_new_scope: bool) -> ScopeId {
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
