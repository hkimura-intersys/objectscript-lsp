use std::collections::HashMap;
use crate::parse_structures::{Class, ClassId, DfsState, GlobalSemanticModel, LocalSemanticModel, LocalSemanticModelId, Method, MethodId, OverrideIndex, VarId, Variable};
use crate::scope_structures::{GlobalSymbol, GlobalSymbolKind, SymbolId};
use tower_lsp::lsp_types::Url;
use tree_sitter::Range;

/*
#[derive(Clone, Debug)]
pub struct LocalSemanticModel {
    pub methods: Vec<Method>,
    pub properties: Vec<ClassProperty>,
    pub variables: Vec<Variable>,
}

pub struct GlobalSemanticModel {
    pub variables: Vec<Variable>,
    pub classes: Vec<Class>,
    pub methods: Vec<Method>,
    pub class_parameters: Vec<ClassParameter>,
    pub class_properties: Vec<ClassProperty>,
    pub private: Vec<LocalSemanticModel>
}
 */

impl GlobalSemanticModel {
    pub fn new() -> Self {
        Self {
            variables: Vec::new(),
            classes: Vec::new(),
            methods: Vec::new(),
            private: Vec::new(),
            defs: Vec::new(),
        }
    }

    pub(crate) fn new_variable(&mut self, variable: Variable) -> VarId {
        let id = VarId(self.variables.len());
        self.variables.push(variable);
        id
    }

    pub fn new_class(&mut self, class: Class) -> ClassId {
        let id = ClassId(self.classes.len());
        self.classes.push(class);
        id
    }

    pub fn new_method(&mut self, method: Method) -> MethodId {
        let id = MethodId(self.methods.len());
        self.methods.push(method);
        id
    }

    pub fn new_local_semantic(
        &mut self,
        local_semantic: LocalSemanticModel,
    ) -> LocalSemanticModelId {
        let id = LocalSemanticModelId(self.private.len());
        self.private.push(local_semantic);
        id
    }

    pub fn new_symbol(
        &mut self,
        name: String,
        kind: GlobalSymbolKind,
        range: Range,
        url: Url,
    ) -> SymbolId {
        let id = SymbolId(self.defs.len());
        self.defs.push(GlobalSymbol {
            name,
            kind,
            url,
            location: range,
        });
        id
    }

    pub fn new_private_method(&mut self, method: Method) -> MethodId {
        let id = MethodId(self.private.len());
        self.methods.push(method);
        id
    }

    /// Build override/dispatch index for PUBLIC methods only.
    ///
    /// IMPORTANT: `class.inherited_classes` must contain *direct* parents only
    /// at the time you call this (do NOT overwrite it with a transitive list first).
    pub fn build_override_index_public_only(&self) -> OverrideIndex {
        #[derive(Clone)]
        struct ClassSnap {
            parents: Vec<ClassId>,                   // direct parents
            inheritance_direction: String,           // "left" or "right"
            public_methods: Vec<(String, MethodId)>, // declared public methods in this class
        }

        // ---- Phase A: snapshot minimal data so DFS doesn't hold locks ----
        let snaps: Vec<ClassSnap> = {
            self.classes
                .iter()
                .map(|c| ClassSnap {
                    parents: c.inherited_classes.clone(), // direct only
                    inheritance_direction: c.inheritance_direction.clone(),
                    public_methods: c
                        .public_methods
                        .iter()
                        .map(|(n, id)| (n.clone(), *id))
                        .collect(),
                })
                .collect()
        };

        let n = snaps.len();
        let mut memo: Vec<Option<HashMap<String, MethodId>>> = vec![None; n];
        let mut state: Vec<DfsState> = vec![DfsState::Unvisited; n];

        let mut index = OverrideIndex::new();

        fn dfs(
            idx: usize,
            snaps: &Vec<ClassSnap>,
            memo: &mut Vec<Option<HashMap<String, MethodId>>>,
            state: &mut Vec<DfsState>,
            index: &mut OverrideIndex,
        ) -> HashMap<String, MethodId> {
            if let Some(cached) = memo[idx].clone() {
                return cached;
            }

            if state[idx] == DfsState::Visiting {
                // Cycle like A->B->C->A.
                panic!("Cycle detected in inheritance graph");
            }

            state[idx] = DfsState::Visiting;

            let cls_id = ClassId(idx);
            let snap = &snaps[idx];

            // Start with inherited effective table
            let mut table: HashMap<String, MethodId> = HashMap::new();

            // Merge parents in precedence order.
            //
            // Strategy: "first wins" merge using entry().or_insert().
            // So for left-inheritance (leftmost wins), iterate parents left->right.
            // For right-inheritance (rightmost wins), iterate parents right->left.
            let parent_iter: Box<dyn Iterator<Item = &ClassId>> =
                if snap.inheritance_direction == "right" {
                    Box::new(snap.parents.iter().rev())
                } else {
                    Box::new(snap.parents.iter())
                };

            for parent in parent_iter {
                let parent_table = dfs(parent.0, snaps, memo, state, index);
                for (name, mid) in parent_table {
                    table.entry(name).or_insert(mid); // first wins
                }
            }

            // Overlay this class’s declared pub methods.
            // If a name already exists in the inherited table => override.
            for (name, child_mid) in &snap.public_methods {
                index.method_owner.insert(*child_mid, cls_id);

                if let Some(base_mid) = table.get(name).copied() {
                    index.overrides.insert(*child_mid, base_mid);
                    index
                        .overridden_by
                        .entry(base_mid)
                        .or_default()
                        .push(*child_mid);
                }

                // child wins
                table.insert(name.clone(), *child_mid);
            }

            state[idx] = DfsState::Done;
            memo[idx] = Some(table.clone());
            index.effective_public_methods.insert(cls_id, table.clone());

            table
        }

        // Compute effective tables for every class
        for i in 0..n {
            let _ = dfs(i, &snaps, &mut memo, &mut state, &mut index);
        }

        index
    }
}
