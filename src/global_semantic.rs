use crate::override_index::OverrideIndex;
use crate::parse_structures::{
    Class, ClassId, DfsState, Language, LocalSemanticModel, LocalSemanticModelId, Method,
    MethodRef, PrivateMethodId, PublicMethodId, PublicMethodRef, PublicVarId, Variable,
};
use crate::scope_structures::{
    ClassGlobalSymbol, ClassGlobalSymbolId, MethodGlobalSymbol, MethodGlobalSymbolId,
    VariableGlobalSymbol, VariableGlobalSymbolId,
};
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;
use tree_sitter::Range;

#[derive(Clone, Debug)]
pub struct GlobalSemanticModel {
    pub variables: HashMap<ClassId, Vec<Variable>>,
    pub classes: Vec<Class>,
    pub methods: HashMap<ClassId, Vec<Method>>,
    pub private: Vec<LocalSemanticModel>,
    pub class_defs: Vec<ClassGlobalSymbol>,
    pub method_defs: HashMap<ClassGlobalSymbolId, Vec<MethodGlobalSymbol>>, // maps class symbol id -> methods symbol id in that class
    pub(crate) variable_defs: HashMap<ClassGlobalSymbolId, Vec<VariableGlobalSymbol>>, // maps class symbol id -> pub variables symbol id in that class
}

impl GlobalSemanticModel {
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

    pub(crate) fn new_variable(&mut self, variable: Variable, class_id: &ClassId) -> PublicVarId {
        let vars = self.variables.entry(class_id.clone()).or_insert(Vec::new());
        let id = PublicVarId(vars.len());
        vars.push(variable);
        id
    }

    pub fn new_class(&mut self, class: Class) -> ClassId {
        let id = ClassId(self.classes.len());
        self.classes.push(class);
        id
    }

    pub fn new_method(&mut self, method: Method, class_id: ClassId) {
        self.methods
            .entry(class_id.clone())
            .or_insert(Vec::new())
            .push(method);
    }

    pub fn new_local_semantic(
        &mut self,
        local_semantic: LocalSemanticModel,
    ) -> LocalSemanticModelId {
        let id = LocalSemanticModelId(self.private.len());
        self.private.push(local_semantic);
        id
    }

    pub fn get_local_semantic_mut(
        &mut self,
        lsm_id: LocalSemanticModelId,
    ) -> Option<&mut LocalSemanticModel> {
        self.private.get_mut(lsm_id.0)
    }

    /// Resets the class struct (removing everything from it), and removes everything from the methods and variables
    /// Note that this function should only be used if the doc is being reparsed, NOT if it is being fully deleted.
    pub fn reset_doc_semantics(
        &mut self,
        class_id: ClassId,
        class_name: String,
        local_semantic_model_id: LocalSemanticModelId,
    ) {
        let class = &mut self.classes[class_id.0];
        class.clear(class_name, true);
        self.methods.remove(&class_id);
        self.variables.remove(&class_id);
        // reset everything in the local semantic model
        self.private[local_semantic_model_id.0].clear()
    }

    pub fn remove_document_symbols(&mut self, class_symbol_id: ClassGlobalSymbolId) {
        let class_symbol = &mut self.class_defs[class_symbol_id.0];
        class_symbol.alive = false;
        // remove all the method and variables symbols defined in given class
        self.method_defs.remove(&class_symbol_id);
        self.variable_defs.remove(&class_symbol_id);
    }

    pub fn update_class_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
        symbol_id: ClassGlobalSymbolId,
    ) {
        let symbol = &mut self.class_defs[symbol_id.0];
        symbol.alive = true;
        symbol.name = name;
        symbol.location = range;
        symbol.url = url;
    }

    pub fn new_class_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
    ) -> ClassGlobalSymbolId {
        let id = ClassGlobalSymbolId(self.class_defs.len());
        self.class_defs.push(ClassGlobalSymbol {
            name,
            url,
            location: range,
            alive: true,
        });
        id
    }

    pub fn new_method_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
        class_symbol_id: ClassGlobalSymbolId,
    ) -> MethodGlobalSymbolId {
        self.method_defs
            .entry(class_symbol_id)
            .or_insert(Vec::new())
            .push(MethodGlobalSymbol {
                name,
                url,
                location: range,
            });
        let id = MethodGlobalSymbolId(self.method_defs[&class_symbol_id].len() - 1);
        id
    }

    pub fn new_variable_symbol(
        &mut self,
        name: String,
        range: Range,
        url: Url,
        var_dependencies: Vec<String>,
        property_dependencies: Vec<String>,
        class_symbol_id: ClassGlobalSymbolId,
    ) -> VariableGlobalSymbolId {
        self.variable_defs
            .entry(class_symbol_id)
            .or_insert(Vec::new())
            .push(VariableGlobalSymbol {
                name,
                url,
                location: range,
                var_dependencies,
                property_dependencies,
            });
        let id = VariableGlobalSymbolId(self.variable_defs[&class_symbol_id].len() - 1);
        id
    }

    pub fn class_keyword_inheritance(&mut self) {
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
    }

    /// Build override/dispatch index for PUBLIC methods only.
    ///
    /// IMPORTANT: `class.inherited_classes` must contain *direct* parents only
    /// at the time you call this (do NOT overwrite it with a transitive list first).
    pub fn build_override_index_public_only(&self) -> OverrideIndex {
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
                        eprintln!("Expected Base Ref to have a Public Method Id");
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

        index
    }
}
