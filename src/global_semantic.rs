use crate::common::{generic_exit_statements, start_of_function, successful_exit};
use crate::override_index::OverrideIndex;
use crate::parse_structures::{
    Class, ClassId, DfsState, Language, LocalSemanticModelId, Method,
    MethodRef, PrivateMethodId, PublicMethodId, PublicMethodRef, PublicVarId, Variable,
};
use crate::scope_structures::{
    ClassGlobalSymbol, ClassGlobalSymbolId, MethodGlobalSymbol, MethodGlobalSymbolId,
    VariableGlobalSymbol, VariableGlobalSymbolId,
};
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;
use tree_sitter::Range;
use crate::local_semantic::LocalSemanticModel;

/// Holds the semantic information and symbols for classes, public methods, and public variables.
#[derive(Clone, Debug)]
pub struct GlobalSemanticModel {
    /// Stores public variables per class.
    pub variables: HashMap<ClassId, Vec<Variable>>,
    /// Stores all classes in a workspace.
    pub classes: Vec<Class>,
    /// Stores public methods per class.
    pub methods: HashMap<ClassId, Vec<Method>>,
    /// Stores all local semantic models in a workspace.
    pub private: Vec<LocalSemanticModel>,
    /// Stores all class symbols in a workspace.
    pub class_defs: Vec<ClassGlobalSymbol>,
    /// Stores Method Global Symbols per Class Global Symbol
    pub method_defs: HashMap<ClassGlobalSymbolId, Vec<MethodGlobalSymbol>>,
    /// Stores Variable Global Symbols per Class Global Symbol
    pub(crate) variable_defs: HashMap<ClassGlobalSymbolId, Vec<VariableGlobalSymbol>>,
}

impl GlobalSemanticModel {
    /// Creates an empty `GlobalSemanticModel` with all tables initialized.
    ///
    /// This initializes storage for classes, methods, variables, local semantic models, and
    /// symbol-definition maps, but does not populate any semantic data.
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            classes: Vec::new(),
            methods: HashMap::new(),
            private: Vec::new(),
            class_defs: Vec::new(),
            method_defs: HashMap::new(),
            variable_defs: HashMap::new(),
        }
    }

    /// Given a Variable, adds the variable to the vec corresponding to the class the variable is defined in.
    /// Returns PublicVarId, which corresponds to the index which the Variable is stored.
    pub(crate) fn new_variable(&mut self, variable: Variable, class_id: &ClassId) -> PublicVarId {
        start_of_function("GlobalSemanticModel", "new_variable");
        let vars = self.variables.entry(class_id.clone()).or_insert(Vec::new());
        let id = PublicVarId(vars.len());
        eprintln!(
            "current variables for associated class are: {sep} {:?} {sep}",
            vars,
            sep = "\n\n"
        );
        eprintln!(
            "Info: Adding variable to global semantic model: {sep} {:?} {sep}",
            variable,
            sep = "\n\n"
        );
        vars.push(variable);
        successful_exit("GlobalSemanticModel", "new_variable");
        id
    }

    /// Given a Class, adds the class to the `self.classes` vec, returning ClassId, which
    /// corresponds to the index that the Class is stored.
    pub fn new_class(&mut self, class: Class) -> ClassId {
        start_of_function("GlobalSemanticModel", "new_class");
        let id = ClassId(self.classes.len());
        let classes = &mut self.classes;
        eprintln!(
            "Info: Current classes in global semantic model are {sep} {:?} {sep}",
            classes,
            sep = "\n\n"
        );
        eprintln!(
            "Info: Adding class {} to global semantic model",
            class.name.clone()
        );
        classes.push(class);
        successful_exit("GlobalSemanticModel", "new_class");
        id
    }

    /// Given a Method, adds the method to the vec corresponding to the class the method is defined in.
    pub fn new_method(&mut self, method: Method, class_id: ClassId) {
        start_of_function("GlobalSemanticModel", "new_method");
        let methods = self.methods.entry(class_id.clone()).or_insert(Vec::new());
        eprintln!(
            "Info: Current public methods for associated class are {sep} {:?} {sep}",
            methods,
            sep = "\n\n"
        );
        eprintln!(
            "Info: Adding method {} to global semantic model",
            method.name.clone()
        );
        methods.push(method);
        successful_exit("GlobalSemanticModel", "new_method");
    }

    /// Appends a new `LocalSemanticModel` to the global store `self.private` and returns its stable id.
    ///
    /// The returned `LocalSemanticModelId` is the index in the internal `private` vector.
    pub fn new_local_semantic(
        &mut self,
        local_semantic: LocalSemanticModel,
    ) -> LocalSemanticModelId {
        start_of_function("GlobalSemanticModel", "new_local_semantic");
        let id = LocalSemanticModelId(self.private.len());
        let lsms = &mut self.private;
        eprintln!(
            "Info: Current local semantic models are {sep} {:?} {sep}",
            lsms,
            sep = "\n\n"
        );
        lsms.push(local_semantic);
        successful_exit("GlobalSemanticModel", "new_local_semantic");
        id
    }

    /// Returns a mutable reference to the local semantic model with the given id.
    ///
    /// Logs a warning and returns `None` if `lsm_id` is out of bounds.
    pub fn get_local_semantic_mut(
        &mut self,
        lsm_id: LocalSemanticModelId,
    ) -> Option<&mut LocalSemanticModel> {
        let result = self.private.get_mut(lsm_id.0);
        start_of_function("GlobalSemanticModel", "get_local_semantic_mut");
        match result {
            None => {
                generic_exit_statements("GlobalSemanticModel", "get_local_semantic_mut");
                eprintln!("Warning: Failed to get local semantic model: Index {:?} out of bounds for local semantic models vector: {sep} {:?} {sep}", lsm_id.0,result, sep= "\n");
                result
            }
            Some(_) => {
                successful_exit("GlobalSemanticModel", "get_local_semantic_mut");
                result
            }
        }
    }

    /// Fetches an immutable reference to a method by `(class_id, index)`.
    ///
    /// Looks up the method vector for `class_id` and then indexes into it. Logs and returns `None`
    /// if the class has no recorded methods or `index` is out of bounds.
    pub fn get_method(&self, class_id: ClassId, class_name: &str, index: usize) -> Option<&Method> {
        start_of_function("GlobalSemanticModel", "get_method");
        let Some(methods) = self.methods.get(&class_id) else {
            eprintln!(
                "Warning: no methods are documented in global semantic model for class named {:?}",
                class_name
            );
            generic_exit_statements("GlobalSemanticModel", "get_method");
            return None;
        };
        let Some(method) = methods.get(index) else {
            eprintln!("Warning: Index {:?} is out of range, failed to get method from the methods vec: {:?} for class named: {:?}", index, methods, class_name);
            generic_exit_statements("GlobalSemanticModel", "get_method");
            return None;
        };
        successful_exit("GlobalSemanticModel", "get_method");
        Some(method)
    }

    /// Returns an immutable reference to the class at `index` in the classes table.
    ///
    /// Logs a warning and returns `None` if `index` is out of bounds.
    pub fn get_class(&self, index: usize) -> Option<&Class> {
        start_of_function("GlobalSemanticModel", "get_class");
        let result = self.classes.get(index);

        match result {
            None => {
                eprintln!("Warning: Index {:?} is out of range, failed to get class from the classes vec: {:?}", index, self.classes);
                generic_exit_statements("GlobalSemanticModel", "get_class");
                result
            }
            Some(_) => {
                successful_exit("GlobalSemanticModel", "get_class");
                result
            }
        }
    }

    /// Returns the `ClassGlobalSymbol` at `index` in the class symbol table.
    ///
    /// Logs a warning and returns `None` if `index` is out of bounds.
    pub fn get_class_symbol(&self, index: usize, class_name: &str) -> Option<&ClassGlobalSymbol> {
        start_of_function("GlobalSemanticModel", "get_class_symbol");
        let Some(class_global_symbol) = self.class_defs.get(index) else {
            eprintln!("Warning: Index {:?} is out of range, failed to get class symbol from the class defs vec: {:?} for class named {:?}", index, self.class_defs, class_name);
            generic_exit_statements("GlobalSemanticModel", "get_class_symbol");
            return None;
        };
        successful_exit("GlobalSemanticModel", "get_class_symbol");
        Some(class_global_symbol)
    }

    /// Fetches a mutable reference to a method by `(class_id, index)`.
    ///
    /// Logs and returns `None` if the class has no recorded methods or `index` is out of bounds.
    pub fn get_mut_method(
        &mut self,
        class_id: ClassId,
        class_name: &str,
        index: usize,
    ) -> Option<&mut Method> {
        start_of_function("GlobalSemanticModel", "get_mut_method");
        let Some(methods) = self.methods.get_mut(&class_id) else {
            eprintln!(
                "Warning: no methods are documented in global semantic model for class named {:?}",
                class_name
            );
            generic_exit_statements("GlobalSemanticModel", "get_mut_method");
            return None;
        };

        if index >= methods.len() {
            eprintln!("Warning: Index {:?} is out of range, failed to get method from the methods vec of len: {:?} for class named: {:?}", index, methods.len(), class_name);
            generic_exit_statements("GlobalSemanticModel", "get_mut_method");
        }
        let Some(method) = methods.get_mut(index) else {
            return None;
        };
        successful_exit("GlobalSemanticModel", "get_mut_method");
        Some(method)
    }

    /// Returns the `MethodGlobalSymbol` for a class symbol by symbol index.
    ///
    /// Logs and returns `None` if the class has no method symbols recorded or `method_symbol_id` is
    /// out of bounds.
    pub fn get_method_symbol(
        &self,
        class_symbol_id: ClassGlobalSymbolId,
        class_name: &str,
        method_symbol_id: usize,
    ) -> Option<&MethodGlobalSymbol> {
        start_of_function("GlobalSemanticModel", "get_method_symbol");
        let Some(method_symbols) = self.method_defs.get(&class_symbol_id) else {
            eprintln!("Warning: no method symbols are documented in global semantic model for class named {:?}", class_name);
            generic_exit_statements("GlobalSemanticModel", "get_method_symbol");
            return None;
        };

        let Some(sym) = method_symbols.get(method_symbol_id) else {
            eprintln!("Warning: Index {:?} is out of range, failed to get method symbol from the method symbols vec: {:?} for class named: {:?}", method_symbol_id, method_symbols, class_name);
            generic_exit_statements("GlobalSemanticModel", "get_method_symbol");
            return None;
        };
        successful_exit("GlobalSemanticModel", "get_method_symbol");
        Some(sym)
    }

    /// Returns an immutable reference to the local semantic model with the given id.
    ///
    /// Logs a warning and returns `None` if `lsm_id` is out of bounds.
    pub fn get_local_semantic(&self, lsm_id: LocalSemanticModelId) -> Option<&LocalSemanticModel> {
        start_of_function("GlobalSemanticModel", "get_local_semantic");
        let result = self.private.get(lsm_id.0);
        match result {
            None => {
                eprintln!("Warning: Failed to get local semantic model: Index {:?} out of bounds for local semantic models vector: {sep} {:?} {sep}", lsm_id.0, self.private, sep= "\n");
                generic_exit_statements("GlobalSemanticModel", "get_local_semantic");
                result
            }
            Some(_) => {
                successful_exit("GlobalSemanticModel", "get_local_semantic");
                result
            }
        }
    }

    /// Returns the `VariableGlobalSymbol` for a class symbol by symbol index.
    ///
    /// Logs and returns `None` if the class has no variable symbols recorded or `index` is out of bounds.
    pub fn get_variable_symbol(
        &self,
        class_symbol_id: &ClassGlobalSymbolId,
        index: usize,
        class_name: &str,
    ) -> Option<&VariableGlobalSymbol> {
        start_of_function("GlobalSemanticModel", "get_variable_symbol");
        let Some(var_symbols) = self.variable_defs.get(class_symbol_id) else {
            eprintln!("Warning: no variable symbols are documented in global semantic model for class named {:?}", class_name);
            generic_exit_statements("GlobalSemanticModel", "get_variable_symbol");
            return None;
        };
        let Some(var_symbol) = var_symbols.get(index) else {
            eprintln!("Warning: Index {:?} is out of range, failed to get variable symbol from the methods vec: {:?} for class named: {:?}", index, var_symbols, class_name);
            generic_exit_statements("GlobalSemanticModel", "get_variable_symbol");
            return None;
        };
        successful_exit("GlobalSemanticModel", "get_variable_symbol");
        Some(var_symbol)
    }

    /// Clears all semantic state associated with a re-parsed document.
    ///
    /// Resets the class entry, removes method/variable tables for `class_id`, and clears the
    /// associated local semantic model. Use this when a document is being reparsed, not deleted.
    pub fn reset_doc_semantics(
        &mut self,
        class_id: ClassId,
        class_name: String,
        local_semantic_model_id: LocalSemanticModelId,
    ) {
        start_of_function("GlobalSemanticModel", "reset_doc_semantics");
        let Some(class) = self.classes.get_mut(class_id.0) else {
            eprintln!("Error: class named {:?} not found", class_name);
            generic_exit_statements("GlobalSemanticModel", "reset_doc_semantics");
            return;
        };
        class.clear(class_name, true);
        self.methods.remove(&class_id);
        self.variables.remove(&class_id);
        // reset everything in the local semantic model
        let Some(local_semantic_model) = self.private.get_mut(local_semantic_model_id.0) else {
            eprintln!("In reset doc semantics, Error: local model not found");
            generic_exit_statements("GlobalSemanticModel", "reset_doc_semantics");
            return;
        };
        local_semantic_model.clear();
        successful_exit("GlobalSemanticModel", "reset_doc_semantics");
    }

    /// Marks the class symbol as inactive and removes all method/variable symbols for the document.
    pub fn remove_document_symbols(&mut self, class_symbol_id: ClassGlobalSymbolId) {
        start_of_function("GlobalSemanticModel", "remove_document_symbols");
        let Some(class_symbol) = self.class_defs.get_mut(class_symbol_id.0) else {
            eprintln!("In remove_document_symbols, Error: class symbol not found");
            generic_exit_statements("GlobalSemanticModel", "remove_document_symbols");
            return;
        };
        class_symbol.alive = false;
        // remove all the method and variables symbols defined in given class
        self.method_defs.remove(&class_symbol_id);
        self.variable_defs.remove(&class_symbol_id);
        successful_exit("GlobalSemanticModel", "remove_document_symbols");
    }

    /// Updates an existing class symbol’s metadata and marks it as alive.
    pub fn update_class_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
        symbol_id: ClassGlobalSymbolId,
    ) {
        start_of_function("GlobalSemanticModel", "update_class_symbol");
        let Some(symbol) = self.class_defs.get_mut(symbol_id.0) else {
            eprintln!("In update_class_symbol, Error: class symbol not found");
            generic_exit_statements("GlobalSemanticModel", "update_class_symbol");
            return;
        };
        symbol.alive = true;
        symbol.name = name;
        symbol.location = range;
        symbol.url = url;
        successful_exit("GlobalSemanticModel", "update_class_symbol");
    }

    /// Creates a new class symbol entry and returns its id.
    pub fn new_class_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
    ) -> ClassGlobalSymbolId {
        start_of_function("GlobalSemanticModel", "new_class_symbol");
        let id = ClassGlobalSymbolId(self.class_defs.len());
        eprintln!("Info: Adding new class symbol for class named {:?}", name);
        self.class_defs.push(ClassGlobalSymbol {
            name,
            url,
            location: range,
            alive: true,
        });
        successful_exit("GlobalSemanticModel", "new_class_symbol");
        id
    }

    /// Adds a new method symbol under `class_symbol_id` and returns its per-class symbol id.
    ///
    /// Returns `None` (and logs) if the per-class method symbol table cannot be retrieved.
    pub fn new_method_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
        class_symbol_id: ClassGlobalSymbolId,
    ) -> Option<MethodGlobalSymbolId> {
        start_of_function("GlobalSemanticModel", "new_method_symbol");
        eprintln!("Info: Adding new public method symbol for method named {:?} for url path {:?}", name, url.path());
        let defs = self.method_defs
            .entry(class_symbol_id)
            .or_insert(Vec::new());
        let id = MethodGlobalSymbolId(defs.len());
        defs.push(MethodGlobalSymbol {
                name,
                url,
                location: range,
            });
        successful_exit("GlobalSemanticModel", "new_method_symbol");
        Some(id)
    }

    /// Adds a new variable symbol under `class_symbol_id` and returns its per-class symbol id.
    ///
    /// Returns `None` (and logs) if the per-class variable symbol table cannot be retrieved.
    pub fn new_variable_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
        var_dependencies: Vec<String>,
        property_dependencies: Vec<String>,
        class_symbol_id: ClassGlobalSymbolId,
    ) -> Option<VariableGlobalSymbolId> {
        start_of_function("GlobalSemanticModel", "new_variable_symbol");
        eprintln!("Info: Adding new public variable symbol for variable named {:?} for url path {:?}", name, url.path());
        let defs = self.variable_defs
            .entry(class_symbol_id)
            .or_insert(Vec::new());
        let id: VariableGlobalSymbolId = VariableGlobalSymbolId(defs.len());
        defs.push(VariableGlobalSymbol {
            name,
            url,
            location: range,
            var_dependencies,
            property_dependencies,
        });
        successful_exit("GlobalSemanticModel", "new_variable_symbol");
        Some(id)
    }

    /// Computes effective class keyword values (procedure block + default language) from inheritance.
    ///
    /// Fills only missing (`None`) values using the primary parent (leftmost) transitively, with
    /// cycle protection via DFS state/memoization.
    pub fn class_keyword_inheritance(&mut self) {
        start_of_function("GlobalSemanticModel", "class_keyword_inheritance");
        #[derive(Clone)]
        struct Snap {
            declared_pb: Option<bool>,
            declared_lang: Option<Language>,
            primary_parent: Option<ClassId>, // leftmost only
        }

        let snaps: Vec<Snap> = self
            .classes
            .iter()
            .map(|c| Snap {
                declared_pb: c.is_procedure_block,
                declared_lang: c.default_language.clone(),
                primary_parent: c.inherited_classes.get(0).copied(),
            })
            .collect();

        let n = snaps.len();
        let mut memo: Vec<Option<(Option<bool>, Option<Language>)>> = vec![None; n];
        let mut state: Vec<DfsState> = vec![DfsState::Unvisited; n];

        fn dfs(
            idx: usize,
            snaps: &Vec<Snap>,
            memo: &mut Vec<Option<(Option<bool>, Option<Language>)>>,
            state: &mut Vec<DfsState>,
        ) -> (Option<bool>, Option<Language>) {
            if let Some(v) = memo[idx].clone() {
                return v;
            }

            if state[idx] == DfsState::Visiting {
                let s = &snaps[idx];
                return (s.declared_pb, s.declared_lang.clone());
            }

            state[idx] = DfsState::Visiting;

            let s = &snaps[idx];

            // start with declared values
            let mut pb = s.declared_pb;
            let mut lang = s.declared_lang.clone();

            // fill missing from primary parent transitively
            if pb.is_none() || lang.is_none() {
                if let Some(parent) = s.primary_parent {
                    if parent.0 < snaps.len() {
                        let (ppb, plang) = dfs(parent.0, snaps, memo, state);
                        if pb.is_none() {
                            pb = ppb;
                        }
                        if lang.is_none() {
                            lang = plang;
                        }
                    }
                }
            }

            state[idx] = DfsState::Done;
            memo[idx] = Some((pb, lang.clone()));
            (pb, lang)
        }

        // ---- Phase B: apply (only fill None) ----
        for i in 0..n {
            let (eff_pb, eff_lang) = dfs(i, &snaps, &mut memo, &mut state);

            let cls = &mut self.classes[i];

            if cls.is_procedure_block.is_none() {
                cls.is_procedure_block = eff_pb;
            }
            if cls.default_language.is_none() {
                cls.default_language = eff_lang;
            }
        }
        successful_exit("GlobalSemanticModel", "class_keyword_inheritance");
    }

    /// Builds an override/dispatch index for methods across the inheritance graph.
    ///
    /// Produces:
    /// - per-class effective public method table,
    /// - override relationships (`overrides` / `overridden_by`) for public and private declarations.
    ///
    /// IMPORTANT: `class.inherited_classes` must contain direct parents only when called.
    pub fn build_override_index(&self) -> OverrideIndex {
        start_of_function("GlobalSemanticModel", "build_override_index");
        #[derive(Clone)]
        struct ClassSnap {
            parents: Vec<ClassId>,
            inheritance_direction: String, // "left" or "right"
            public_methods: Vec<(String, PublicMethodId)>, // declared public methods in this class
            private_methods: Vec<(String, PrivateMethodId)>,
        }

        let snaps: Vec<ClassSnap> = self
            .classes
            .iter()
            .map(|c| ClassSnap {
                parents: c.inherited_classes.clone(),
                inheritance_direction: c.inheritance_direction.clone(),
                public_methods: c
                    .public_methods
                    .iter()
                    .map(|(n, id)| (n.clone(), *id))
                    .collect(),
                private_methods: c
                    .private_methods
                    .iter()
                    .map(|(n, id)| (n.clone(), *id))
                    .collect(),
            })
            .collect();

        let n = snaps.len();
        let mut memo: Vec<Option<HashMap<String, MethodRef>>> = vec![None; n];
        let mut state: Vec<DfsState> = vec![DfsState::Unvisited; n];
        let mut index = OverrideIndex::new();

        fn dfs(
            idx: usize,
            snaps: &Vec<ClassSnap>,
            memo: &mut Vec<Option<HashMap<String, MethodRef>>>,
            state: &mut Vec<DfsState>,
            index: &mut OverrideIndex,
        ) -> HashMap<String, MethodRef> {
            if let Some(cached) = memo[idx].clone() {
                return cached;
            }
            if state[idx] == DfsState::Visiting {
                eprintln!("Cycle detected in inheritance graph");
                generic_exit_statements("GlobalSemanticModel", "build_override_index");
                return HashMap::new();
            }

            state[idx] = DfsState::Visiting;

            let cls_id = ClassId(idx);
            let snap = &snaps[idx];

            // inherited effective table
            let mut table: HashMap<String, MethodRef> = HashMap::new();

            let parent_iter: Box<dyn Iterator<Item = &ClassId>> =
                if snap.inheritance_direction == "right" {
                    Box::new(snap.parents.iter().rev())
                } else {
                    Box::new(snap.parents.iter())
                };

            for parent in parent_iter {
                let parent_table = dfs(parent.0, snaps, memo, state, index);
                for (name, mref) in parent_table {
                    table.entry(name).or_insert(mref); // first wins
                }
            }

            // overlay declared methods for this class
            for (name, child_mid) in &snap.public_methods {
                let child_ref = MethodRef {
                    class: cls_id,
                    pub_id: Some(*child_mid),
                    priv_id: None,
                };

                if let Some(base_ref) = table.get(name).copied() {
                    if let Some(base_pid) = base_ref.pub_id {
                        let base_pub = PublicMethodRef {
                            class: base_ref.class,
                            id: base_pid,
                        };
                        index.overrides.insert(child_ref, base_pub);
                        index
                            .overridden_by
                            .entry(base_pub)
                            .or_default()
                            .push(child_ref);
                    }
                }

                table.insert(name.clone(), child_ref); // child wins
            }

            for (name, child_mid) in &snap.private_methods {
                let child_ref = MethodRef {
                    class: cls_id,
                    pub_id: None,
                    priv_id: Some(*child_mid),
                };

                if let Some(base_ref) = table.get(name).copied() {
                    let Some(id) = base_ref.pub_id else {
                        eprintln!("Warning: Expected Base Ref to have a Public Method Id");
                        generic_exit_statements("GlobalSemanticModel", "build_override_index");
                        return table;
                    };
                    let base_ref = PublicMethodRef {
                        class: base_ref.class,
                        id,
                    };
                    index.overrides.insert(child_ref, base_ref);
                    index
                        .overridden_by
                        .entry(base_ref)
                        .or_default()
                        .push(child_ref);
                }

                table.insert(name.clone(), child_ref); // child wins
            }

            let effective_public: HashMap<String, PublicMethodRef> = table
                .iter()
                .filter_map(|(name, mref)| {
                    mref.pub_id.map(|pid| {
                        (
                            name.clone(),
                            PublicMethodRef {
                                class: mref.class,
                                id: pid,
                            },
                        )
                    })
                })
                .collect();

            index
                .effective_public_methods
                .insert(cls_id, effective_public);

            state[idx] = DfsState::Done;
            memo[idx] = Some(table.clone());
            table
        }

        for i in 0..n {
            let _ = dfs(i, &snaps, &mut memo, &mut state, &mut index);
        }
        successful_exit("GlobalSemanticModel", "build_override_index");
        index
    }
}
