use crate::document::Document;
use crate::scope_tree::{ScopeTree};
use parking_lot::RwLock;
use std::collections::{HashMap};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tower_lsp::lsp_types::Url;
use crate::semantic::{GlobalSemanticModel};
use crate::parse_structures::{Class, FileType, Language};

pub struct ProjectState {
    pub(crate) project_root_path: OnceLock<Option<PathBuf>>, //should only ever be set on initialize()
    pub(crate) documents: Arc<RwLock<HashMap<Url, Document>>>,
    pub(crate) defs: Arc<RwLock<HashMap<Url, ScopeTree>>>,
    pub(crate) global_semantic_model: Arc<RwLock<GlobalSemanticModel>>,
}

#[derive(Clone)]
struct ClassInfo {
    url: Url,
    declared_procedure_block: Option<bool>,
    declared_lang: Option<Language>,
    primary_parent: Option<String>, // leftmost superclass only
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum VisitState { Visiting, Done }

fn resolve_effective(
    name: &str,
    graph: &HashMap<String, ClassInfo>,
    memo: &mut HashMap<String, (Option<bool>, Option<Language>)>,
    state: &mut HashMap<String, VisitState>,
) -> (Option<bool>, Option<Language>) {
    if let Some(v) = memo.get(name) {
        return v.clone();
    }

    // cycle detection
    if let Some(VisitState::Visiting) = state.get(name).copied() {
        // cycle: safest fallback is "declared only" (don’t recurse)
        if let Some(info) = graph.get(name) {
            let v = (info.declared_procedure_block, info.declared_lang.clone());
            memo.insert(name.to_string(), v.clone());
            return v;
        } else {
            return (None, None);
        }
    }

    let Some(info) = graph.get(name) else {
        return (None, None);
    };

    state.insert(name.to_string(), VisitState::Visiting);

    // start with declared values
    let mut is_procedure_block = info.declared_procedure_block;
    let mut lang = info.declared_lang.clone();

    // fill missing from parent transitively
    if is_procedure_block.is_none() || lang.is_none() {
        if let Some(parent) = info.primary_parent.as_deref() {
            if graph.contains_key(parent) {
                let (ppb, plang) = resolve_effective(parent, graph, memo, state);
                if is_procedure_block.is_none() { is_procedure_block = ppb; }
                if lang.is_none() { lang = plang; }
            }
        }
    }

    state.insert(name.to_string(), VisitState::Done);
    memo.insert(name.to_string(), (is_procedure_block, lang.clone()));
    (is_procedure_block, lang)
}


impl ProjectState {
    pub fn new() -> Self {
        Self {
            project_root_path: OnceLock::new(),
            documents: Arc::new(RwLock::new(HashMap::new())),
            defs: Arc::new(RwLock::new(HashMap::new())),
            global_semantic_model: Arc::new(RwLock::new(GlobalSemanticModel::new())),
        }
    }

    pub fn add_document(&self, url: Url, mut document: Document) {
        if matches!(document.file_type.clone(), FileType::Cls) {
            let class = document.initial_build(document.tree.clone().root_node());
            self.defs.write().insert(url.clone(),document.clone().scope_tree.unwrap());
            self.documents.write().insert(url.clone(), document);
            self.global_semantic_model
                .write()
                .classes
                .insert(class.name.clone(), url.clone());
            self.global_updates(class);
        }
    }

    pub fn root_path(&self) -> Option<&std::path::Path> {
        self.project_root_path.get().and_then(|o| o.as_deref())
    }

    /// After all docs have been parsed, this function is called.
    /// This goes through each class, and updates the classes default language
    /// and default procedure block settings if it isn't yet declared
    /// NOTE: Even with right-to-left inheritance, the leftmost superclass
    /// (sometimes known as the first superclass) is still the primary superclass.
    /// This means that the subclass inherits only the class keyword values of its leftmost
    /// superclass — there is no override for this behavior.
    ///

    pub fn global_update_inherited_classes(&self) {
        // ---- Phase 0: snapshot into an owned graph ----
        let graph: HashMap<String, ClassInfo> = {
            let docs = self.documents.read();
            // holds class info for each class
            let mut workspace_class_info = HashMap::new();

            for (_url, doc) in docs.iter() {
                let Some(lsm) = doc.local_semantic_model.as_ref() else { continue; };
                let class = &lsm.class;

                // class.name is your key
                let name = class.name.clone();

                // leftmost superclass only
                let primary_parent = class.inherited_classes.get(0).cloned();

                workspace_class_info.insert(name, ClassInfo {
                    url: doc.uri.clone(), // if uri is private, store the map key Url instead (see note below)
                    declared_procedure_block: class.is_procedure_block,
                    declared_lang: class.default_language.clone(),
                    primary_parent,
                });
            }

            workspace_class_info
        };

        // ---- Phase 1: resolve effective values with DFS+memo ----
        let mut memo: HashMap<String, (Option<bool>, Option<Language>)> = HashMap::new();
        let mut classes_visit_state: HashMap<String, VisitState> = HashMap::new();

        // We’ll produce per-URL updates for docs that are missing values
        let mut updates: Vec<(Url, Option<bool>, Option<Language>)> = Vec::new();

        for (name, info) in graph.iter() {
            let (eff_pb, eff_lang) = resolve_effective(name, &graph, &mut memo, &mut classes_visit_state);

            // only update if the doc itself is missing
            let new_pb = if info.declared_procedure_block.is_none() { eff_pb } else { None };
            let new_lang = if info.declared_lang.is_none() { eff_lang } else { None };

            if new_pb.is_some() || new_lang.is_some() {
                updates.push((info.url.clone(), new_pb, new_lang));
            }
        }

        // ---- Phase 2: apply updates ----
        let mut docs = self.documents.write();
        for (url, new_pb, new_lang) in updates {
            if let Some(doc) = docs.get_mut(&url) {
                if let Some(lsm) = doc.local_semantic_model.as_mut() {
                    if lsm.class.is_procedure_block.is_none() {
                        if new_pb.is_some() {
                            lsm.class.is_procedure_block = new_pb;
                        }
                    }
                    if lsm.class.default_language.is_none() {
                        if new_lang.is_some() {
                            lsm.class.default_language = new_lang;
                        }
                    }
                }
            }
        }
    }


    /// Adds public local vars, globals, and subclasses.
    /// this is done after the local semantic model for the given class
    /// has finished building.
    pub fn global_updates(&self, mut class: Class) {
        let mut global_semantic_model = self.global_semantic_model.write();

        for inherited_class in class.inherited_classes.clone() {
            global_semantic_model.add_subclass(inherited_class.clone(),class.name.clone());
        }

        drop(global_semantic_model);
        // TODO: GLOBALS AND PUBLIC VARIABLES


    }
}
