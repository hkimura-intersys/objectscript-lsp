use crate::common::{generic_exit_statements, point_in_range, start_of_function, successful_exit};
use crate::scope_structures::*;
use std::collections::HashMap;
use tree_sitter::{Point, Range};

/// A lexical scope within a document.
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    /// Start Point of Scope.
    pub(crate) start: Point,
    /// End Point of Scope.
    pub(crate) end: Point,
    /// Optional: Id of Parent Scope.
    pub(crate) parent: Option<ScopeId>,
    /// Ids of Child Scopes.
    pub(crate) children: Vec<ScopeId>,
    /// Stores the Variable Symbols defined in this scope.
    pub(crate) variable_symbols: Vec<VariableSymbol>,
    /// Stores variable name -> VariableGlobalSymbolId(Index) for public variables defined in this scope.
    pub(crate) public_var_defs: HashMap<String, VariableGlobalSymbolId>,
    /// Stores variable name -> VariableSymbolId(Index) for private variables defined in this scope.
    pub(crate) private_variable_defs: HashMap<String, VariableSymbolId>,
}
impl Scope {
    /// Create a new scope node with the given bounds and optional parent.
    fn new(start: Point, end: Point, parent: Option<ScopeId>) -> Self {
        Self {
            start,
            end,
            parent,
            children: Vec::new(),
            variable_symbols: Vec::new(),
            public_var_defs: HashMap::new(), // HashMap var name -> GlobalSymbol
            private_variable_defs: HashMap::new(),
        }
    }

    /// Look up a private variable definition in this scope by name and return its source range.
    ///
    /// Logs a warning and returns `None` if the name is not present or the stored symbol id is
    /// out of bounds for `variable_symbols`.
    fn get_variable_symbol(&self, variable_name: &str) -> Option<Range> {
        start_of_function("Scope", "get_variable_symbol");
        let Some(variable_symbol_id) = self.private_variable_defs.get(variable_name) else {
            eprintln!(
                "Warning: Couldn't find variable name: {:?} in private variable defs hashmap: {:?}",
                variable_name, self.private_variable_defs
            );
            return None;
        };

        let Some(variable_symbol) = self.variable_symbols.get(variable_symbol_id.0) else {
            eprintln!("Warning: Failed to get variable symbol for variable named {:?}. Index {:?} is out of range in this scopes variable symbols vec: {:?}", variable_name, variable_symbol_id.0, self.variable_symbols);
            generic_exit_statements("Scope", "get_variable_symbol");
            return None;
        };

        successful_exit("Scope", "get_variable_symbol");
        Some(variable_symbol.location)
    }

    /// Define a new private variable symbol in this scope and return its `VariableSymbolId`.
    fn new_variable_symbol(
        &mut self,
        name: String,
        location: Range,
        var_dependencies: Vec<String>,
        property_dependencies: Vec<String>,
    ) -> VariableSymbolId {
        start_of_function("Scope", "new_variable_symbol");
        let sym_id = VariableSymbolId(self.variable_symbols.len());
        self.private_variable_defs.insert(name.clone(), sym_id);
        self.variable_symbols.push(VariableSymbol {
            name: name.clone(),
            location,
            references: Vec::new(),
            var_dependencies,
            property_dependencies,
        });
        successful_exit("Scope", "new_variable_symbol");
        sym_id
    }

    /// Record a public variable definition in this scope by mapping its name to a global symbol id.
    pub fn new_symbol_pub_variable(&mut self, name: String, id: VariableGlobalSymbolId) {
        start_of_function("Scope", "new_symbol_pub_variable");
        self.public_var_defs.insert(name.clone(), id);
        successful_exit("Scope", "new_symbol_pub_variable");
    }

    /// Look up a public variable definition in this scope by name.
    ///
    /// Logs a warning and returns `None` if the name is not present.
    pub fn get_pub_variable_symbol(&self, name: &str) -> Option<VariableGlobalSymbolId> {
        start_of_function("Scope", "get_pub_variable_symbol");
        let Some(&var_global_symbol_id) = self.public_var_defs.get(name) else {
            eprintln!("Warning: Failed to find variable symbol for var named {:?} in this scope's public var defs: {:?}", name, self.public_var_defs);
            generic_exit_statements("Scope", "get_pub_variable_symbol");
            return None;
        };
        successful_exit("Scope", "get_pub_variable_symbol");
        Some(var_global_symbol_id)
    }
}

/// Per-document scope index used for symbol lookup and resolution.
#[derive(Debug)]
pub struct ScopeTree {
    /// Stores ScopeId -> Scope for all Scopes in the document.
    pub scopes: HashMap<ScopeId, Scope>,
    /// The root ScopeId, which spans the whole document.
    pub(crate) root: ScopeId,
    /// The iterator that keeps track of the Id to assign to the next scope.
    pub(crate) next_scope_id: usize,
    /// Stores method name -> Method Symbol for all private methods in the document.
    private_method_defs: HashMap<String, MethodSymbol>,
    /// The Id corresponding to the class definition symbol for this document.
    pub(crate) class_def: ClassGlobalSymbolId,
}

impl Clone for ScopeTree {
    /// Clone the ScopeTree
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
    /// Create a new scope tree with a single root scope spanning the entire document.
    pub fn new(class_symbol_id: ClassGlobalSymbolId) -> Self {
        let root_id = ScopeId(0);
        let root_scope = Scope::new(
            Point { row: 0, column: 0 },
            Point {
                row: usize::MAX,
                column: usize::MAX,
            },
            None,
        );
        let mut scopes = HashMap::new();
        scopes.insert(root_id, root_scope);
        Self {
            scopes,
            root: root_id,
            next_scope_id: 1,
            private_method_defs: HashMap::new(),
            class_def: class_symbol_id,
        }
    }

    /// If `var_name` is a public variable visible at `pos`, return its owning class symbol id and
    /// the variable's global symbol id.
    pub fn pub_variable_in_scope(
        &self,
        pos: Point,
        var_name: &str,
    ) -> Option<(ClassGlobalSymbolId, VariableGlobalSymbolId)> {
        start_of_function("Scope", "pub_variable_in_scope");
        let Some(scope) = self.get_scope(pos) else {
            generic_exit_statements("Scope", "pub_variable_in_scope");
            return None;
        };

        let Some(var_symbol) = scope.get_pub_variable_symbol(var_name) else {
            generic_exit_statements("Scope", "pub_variable_in_scope");
            return None;
        };

        successful_exit("Scope", "pub_variable_in_scope");
        Some((self.class_def, var_symbol))
    }

    /// Look up a private method symbol by name.
    ///
    /// Logs a warning and returns `None` if it does not exist.
    pub fn get_private_method_symbol(&self, name: &str) -> Option<&MethodSymbol> {
        start_of_function("Scope", "get_private_method_symbol");
        let result = self.private_method_defs.get(name);
        match result {
            None => {
                eprintln!("Warning: Failed to get Private Method Symbol Id: No private method named {:?} exists in Scope Tree private methods hashMap: {sep} {:?} {sep}", name, self.private_method_defs, sep= "\n");
                generic_exit_statements("Scope", "get_private_method_symbol");
                result
            }
            Some(_) => {
                successful_exit("Scope", "get_private_method_symbol");
                result
            }
        }
    }

    /// Add a new child scope to `parent`, returning the new `ScopeId`.
    pub fn add_scope(
        &mut self,
        start: Point,
        end: Point,
        parent: ScopeId,
        is_new_scope: bool,
    ) -> ScopeId {
        start_of_function("Scope", "add_scope");
        let scope_id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;
        let scope = Scope {
            start,
            end,
            parent: Some(parent),
            children: Vec::new(),
            variable_symbols: Vec::new(),
            public_var_defs: HashMap::new(),
            private_variable_defs: HashMap::new(),
        };
        // update parent to include this scope as a child
        if let Some(parent_scope) = self.scopes.get_mut(&parent) {
            parent_scope.children.push(scope_id);
        }
        self.scopes.insert(scope_id, scope);
        successful_exit("Scope", "add_scope");
        scope_id
    }

    /// Register a private method definition symbol in this document.
    pub fn new_method_symbol(&mut self, name: String, range: Range) {
        start_of_function("Scope", "new_method_symbol");
        let method_symbol = MethodSymbol {
            name: name.clone(),
            location: range,
        };
        self.private_method_defs.insert(name, method_symbol);
        successful_exit("Scope", "new_method_symbol");
    }

    /// Define a private variable symbol in the scope that contains `range.start_point`.
    ///
    /// Returns the created `VariableSymbolId`, or `None` if no scope contains the start point.
    pub fn new_variable_symbol(
        &mut self,
        name: String,
        range: Range,
        var_deps: Vec<String>,
        prop_deps: Vec<String>,
    ) -> Option<VariableSymbolId> {
        start_of_function("Scope", "new_variable_symbol");
        let Some(scope) = self.get_mut_scope(range.start_point) else {
            generic_exit_statements("Scope", "new_variable_symbol");
            return None;
        };
        let sym_id = scope.new_variable_symbol(name.clone(), range, var_deps, prop_deps);
        successful_exit("Scope", "new_variable_symbol");
        Some(sym_id)
    }

    /// Get a mutable reference to the innermost scope containing `point`.
    ///
    /// Logs a warning and returns `None` if no containing scope is found.
    fn get_mut_scope(&mut self, point: Point) -> Option<&mut Scope> {
        let Some(scope_id) = self.find_current_scope(point) else {
            eprintln!("Warning: Scope Id not found for Point {:?}", point);
            return None;
        };

        let scopes = self.scopes.clone();
        let Some(scope) = self.scopes.get_mut(&scope_id) else {
            eprintln!(
                "Warning: Scope not found, Scope Id {:?} DNE in scopes hashmap: \n {:?} \n\n",
                scope_id, scopes
            );
            return None;
        };
        Some(scope)
    }

    /// Get an immutable reference to the innermost scope containing `point`.
    ///
    /// Logs a warning and returns `None` if no containing scope is found.
    fn get_scope(&self, point: Point) -> Option<&Scope> {
        let Some(scope_id) = self.find_current_scope(point) else {
            eprintln!("Warning: Scope Id not found for Point {:?}", point);
            return None;
        };
        let Some(scope) = self.scopes.get(&scope_id) else {
            eprintln!(
                "Warning: Scope not found, Scope Id {:?} DNE in scopes hashmap: \n {:?} \n\n",
                scope_id, self.scopes
            );
            return None;
        };

        Some(scope)
    }

    /// Record a public variable symbol in the scope that contains `range.start_point`.
    pub fn new_public_var_symbol(
        &mut self,
        name: String,
        range: Range,
        symbol_id: VariableGlobalSymbolId,
    ) {
        start_of_function("Scope", "new_public_var_symbol");
        let Some(scope) = self.get_mut_scope(range.start_point) else {
            generic_exit_statements("Scope", "new_public_var_symbol");
            return;
        };
        scope.new_symbol_pub_variable(name.clone(), symbol_id);
        successful_exit("Scope", "new_public_var_symbol");
    }

    /// Look up a private variable definition visible at `pos` by name.
    pub fn get_variable_definition(&self, pos: Point, variable_name: &str) -> Option<Range> {
        start_of_function("Scope", "get_variable_definition");
        let Some(scope) = self.get_scope(pos) else {
            generic_exit_statements("Scope", "get_variable_definition");
            return None;
        };

        let result = scope.get_variable_symbol(variable_name);
        match result {
            Some(_) => {
                successful_exit("Scope", "get_variable_definition");
                result
            }
            None => {
                generic_exit_statements("Scope", "get_variable_definition");
                result
            }
        }
    }

    /// Find the innermost scope containing `pos` by descending from the root into matching children.
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
