use crate::common::{get_class_name_from_root, get_node_children};
use crate::document::Document;
use crate::method::build_method_calls;
use crate::parse_structures::{
    Class, ClassId, FileType, GlobalSemanticModel, LocalSemanticModel, LocalSemanticModelId,
    Method, MethodCallSite, OverrideIndex,
};
use crate::scope_structures;
use parking_lot::RwLock;
use scope_structures::{GlobalSymbolId, GlobalSymbolKind, SymbolKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tower_lsp::lsp_types::Url;

pub struct ProjectState {
    pub(crate) project_root_path: OnceLock<Option<PathBuf>>, //should only ever be set on initialize()
    pub(crate) documents: Arc<RwLock<HashMap<Url, Document>>>,
    pub(crate) global_semantic_model: Arc<RwLock<GlobalSemanticModel>>,
    pub(crate) classes: Arc<RwLock<HashMap<String, ClassId>>>,
    pub(crate) local_semantic_models: Arc<RwLock<HashMap<Url, LocalSemanticModelId>>>,
    pub(crate) class_defs: Arc<RwLock<HashMap<String, GlobalSymbolId>>>,
    // keyed by Method name, Class name
    pub(crate) public_method_defs: Arc<RwLock<HashMap<(String, String), GlobalSymbolId>>>,
    // TODO: once VarType is solid, we can also split this by VarType -> HashMap<(String, VarType), Vec<GlobalSymbolId>>
    pub(crate) public_variable_defs: Arc<RwLock<HashMap<String, Vec<GlobalSymbolId>>>>,
    pub(crate) override_index: Arc<RwLock<OverrideIndex>>,
}

impl ProjectState {
    pub fn new() -> Self {
        Self {
            project_root_path: OnceLock::new(),
            documents: Arc::new(RwLock::new(HashMap::new())),
            global_semantic_model: Arc::new(RwLock::new(GlobalSemanticModel::new())),
            classes: Arc::new(RwLock::new(HashMap::new())),
            local_semantic_models: Arc::new(RwLock::new(HashMap::new())),
            class_defs: Arc::new(RwLock::new(HashMap::new())),
            public_method_defs: Arc::new(RwLock::new(HashMap::new())),
            public_variable_defs: Arc::new(RwLock::new(HashMap::new())),
            override_index: Arc::new(RwLock::new(OverrideIndex::new())),
        }
    }

    pub fn add_document(&self, url: Url, document: Document, class_name: String) {
        if matches!(document.file_type.clone(), FileType::Cls) {
            // create class struct
            let mut class = Class::new(class_name.clone());
            let mut local_semantic_model = LocalSemanticModel::new();
            // get class def node
            let node = document
                .tree
                .root_node()
                .named_child(document.tree.root_node().named_child_count() - 1)
                .unwrap();
            let class_range = node.range();
            let content = document.content.as_str();
            let methods = class.initial_build(node, content);
            self.documents.write().insert(url.clone(), document);
            let mut global_semantic_model = self.global_semantic_model.write();
            for (method, range) in methods {
                let method_name = method.name.clone();
                if method.is_public {
                    // add method to global semantic model
                    let method_id = global_semantic_model.new_method(method);
                    // add methodId to class public methods field
                    class.public_methods.insert(method_name.clone(), method_id);
                    // create method  global symbol
                    let symbol_id = global_semantic_model.new_symbol(
                        method_name.clone(),
                        GlobalSymbolKind::Method,
                        range,
                        url.clone(),
                        Vec::new(),
                        Vec::new(),
                    );
                    // add method symbol id to project state
                    self.public_method_defs
                        .write()
                        .insert((method_name.clone(), class_name.clone()), symbol_id);
                    let mut docs = self.documents.write();
                    let doc = docs.get_mut(&url).expect("missing doc");
                    // this creates the symbol and adds the symbol id to the scope tree
                    doc.scope_tree.new_public_symbol(
                        method_name.clone(),
                        GlobalSymbolKind::Method,
                        range,
                        symbol_id,
                    );
                    drop(docs);
                } else {
                    // add method to local semantic model
                    let method_id = local_semantic_model.new_method(method);
                    // add methodId to class private methods field
                    class.private_methods.insert(method_name.clone(), method_id);
                    // find current scope and build symbol and add it to the scope
                    let mut docs = self.documents.write();
                    let doc = docs.get_mut(&url).expect("missing doc");
                    // this creates the symbol and adds the symbol id to the scope tree
                    doc.scope_tree.new_symbol(
                        method_name.clone(),
                        SymbolKind::Method,
                        range,
                        Vec::new(),
                        Vec::new(),
                    );
                    drop(docs);
                }
            }
            // add class to global semantic model
            let class_id = global_semantic_model.new_class(class);
            // add class symbol to global semantic model
            // create class global symbol
            let symbol_id = global_semantic_model.new_symbol(
                class_name.clone(),
                GlobalSymbolKind::Class,
                class_range,
                url.clone(),
                Vec::new(),
                Vec::new(),
            );
            self.classes.write().insert(class_name.clone(), class_id);
            // add class symbol id to project state
            self.class_defs
                .write()
                .insert(class_name.clone(), symbol_id);
            let local_semantic_id = global_semantic_model.new_local_semantic(local_semantic_model);
            drop(global_semantic_model);
            self.local_semantic_models
                .write()
                .insert(url, local_semantic_id);
        }
    }

    pub fn root_path(&self) -> Option<&std::path::Path> {
        self.project_root_path.get().and_then(|o| o.as_deref())
    }

    pub fn build_inheritance_and_variables(&self) {
        let documents = self.documents.read().values().cloned().collect::<Vec<_>>();
        for document in documents {
            self.add_class_imports(&document);
            self.add_direct_inherited_class_ids(&document);
        }
        let mut gsm = self.global_semantic_model.write();
        let mut docs = self.documents.write();
        gsm.class_keyword_inheritance();
        let override_index = gsm.build_override_index_public_only();

        *self.override_index.write() = override_index;
        for i in 0..gsm.classes.len() {
            let (class_name, public_method_ids, private_method_ids) = {
                let class = &gsm.classes[i];
                (
                    class.name.clone(),
                    class.public_methods.values().cloned().collect::<Vec<_>>(),
                    class.private_methods.values().cloned().collect::<Vec<_>>(),
                )
            };
            for pub_method_id in public_method_ids {
                let (method_name, url, loc) = {
                    let m = &gsm.methods[pub_method_id.0];
                    let sym_id = self
                        .public_method_defs
                        .read()
                        .get(&(m.name.clone(), class_name.clone()))
                        .unwrap()
                        .clone();
                    let sym = &gsm.defs[sym_id.0];
                    (m.name.clone(), sym.url.clone(), sym.location)
                };

                let tree_root_node = docs.get(&url).unwrap().tree.root_node();
                let method_definition_node = tree_root_node
                    .named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                    .unwrap();
                let content = docs.get(&url).unwrap().content.as_str();

                let calls = build_method_calls(&class_name, method_definition_node, content);
                let new_sites: Vec<MethodCallSite> = calls
                    .into_iter()
                    .map(|call| {
                        let callee_symbol = self
                            .public_method_defs
                            .read()
                            .get(&(call.callee_method.clone(), call.callee_class.clone()))
                            .copied();

                        MethodCallSite {
                            caller_method: method_name.clone(),
                            callee_class: call.callee_class,
                            callee_method: call.callee_method,
                            callee_symbol,
                            call_range: call.call_range,
                            arg_ranges: call.arg_ranges,
                        }
                    })
                    .collect();
                gsm.classes
                    .get_mut(i)
                    .expect("missing class")
                    .method_calls
                    .extend(new_sites);
                // build variables
                //Vec<(Variable, Range, Vec<String>,Vec<String>)
                let result = {
                    let method = &gsm.methods[pub_method_id.0];
                    method.build_method_variables_and_ref(method_definition_node, content)
                };
                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in result {
                    let doc = docs.get_mut(&url).expect("missing doc");
                    let var_name = variable.name.clone();
                    if variable.is_public {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_id = gsm.new_variable(variable);
                            gsm.methods[pub_method_id.0]
                                .public_variables
                                .insert(var_name.clone(), var_id);
                            let symbol_id = gsm.new_symbol(
                                var_name.clone(),
                                GlobalSymbolKind::PubVar,
                                variable_range,
                                url.clone(),
                                refs_to_other_vars.clone(),
                                refs_to_properties.clone(),
                            );
                            doc.scope_tree.new_public_symbol(
                                var_name.clone(),
                                GlobalSymbolKind::PubVar,
                                variable_range,
                                symbol_id,
                            );
                            self.public_variable_defs
                                .write()
                                .entry(var_name.clone())
                                .or_insert_with(Vec::new)
                                .push(symbol_id);
                        }
                    } else {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_name = variable.name.clone();
                            let var_id = {
                                let lsm_id = *self.local_semantic_models.read().get(&url).unwrap();
                                let lsm = gsm.get_local_semantic_mut(lsm_id).unwrap();
                                lsm.new_variable(variable)
                            };
                            gsm.methods[pub_method_id.0]
                                .private_variables
                                .insert(var_name.clone(), var_id);
                            doc.scope_tree.new_symbol(
                                var_name.clone(),
                                SymbolKind::PrivVar,
                                variable_range,
                                Vec::new(),
                                Vec::new(),
                            );
                        }
                    }
                }
            }

            for private_method_id in private_method_ids {
                let (method_name, url, loc) = {
                    let class_symbol_idx = self.class_defs.read().get(&class_name).unwrap().0;
                    let url = gsm.defs[class_symbol_idx].url.clone();
                    let local_semantic_id =
                        self.local_semantic_models.read().get(&url).unwrap().clone();
                    let m = &gsm.private[local_semantic_id.0].methods[private_method_id.0];
                    // I need to get the sym id from the scope tree
                    let scope_tree = &docs.get(&url).expect("missing doc").scope_tree;
                    let (scope_id, sym_id) = scope_tree
                        .get_private_method_symbol(&m.name)
                        .expect("missing private method symbol");
                    let sym = scope_tree
                        .scopes
                        .read()
                        .get(&scope_id)
                        .expect("missing scope")
                        .symbols[sym_id.0]
                        .clone();
                    (m.name.clone(), url, sym.location)
                };

                let tree_root_node = docs.get(&url).unwrap().tree.root_node();
                let method_definition_node = tree_root_node
                    .named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                    .unwrap();
                let content = docs.get(&url).unwrap().content.as_str();
                let calls = build_method_calls(&class_name, method_definition_node, content);
                let new_sites: Vec<MethodCallSite> = calls
                    .into_iter()
                    .map(|call| {
                        let callee_symbol = self
                            .public_method_defs
                            .read()
                            .get(&(call.callee_method.clone(), call.callee_class.clone()))
                            .copied();

                        MethodCallSite {
                            caller_method: method_name.clone(),
                            callee_class: call.callee_class,
                            callee_method: call.callee_method,
                            callee_symbol,
                            call_range: call.call_range,
                            arg_ranges: call.arg_ranges,
                        }
                    })
                    .collect();
                gsm.classes
                    .get_mut(i)
                    .expect("missing class")
                    .method_calls
                    .extend(new_sites);
                // build variables
                let result = {
                    let method = &gsm.methods[private_method_id.0];
                    method.build_method_variables_and_ref(method_definition_node, content)
                };
                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in result {
                    let doc = docs.get_mut(&url).expect("missing doc");
                    let var_name = variable.name.clone();
                    let local_semantic_model_id =
                        self.local_semantic_models.read().get(&url).unwrap().clone();
                    if variable.is_public {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_id = gsm.new_variable(variable);
                            gsm.private[local_semantic_model_id.0].methods[private_method_id.0]
                                .public_variables
                                .insert(var_name.clone(), var_id);
                            let symbol_id = gsm.new_symbol(
                                var_name.clone(),
                                GlobalSymbolKind::PubVar,
                                variable_range,
                                url.clone(),
                                refs_to_other_vars.clone(),
                                refs_to_properties.clone(),
                            );
                            doc.scope_tree.new_public_symbol(
                                var_name.clone(),
                                GlobalSymbolKind::PubVar,
                                variable_range,
                                symbol_id,
                            );
                            self.public_variable_defs
                                .write()
                                .entry(var_name.clone())
                                .or_insert_with(Vec::new)
                                .push(symbol_id);
                        }
                    } else {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_name = variable.name.clone();
                            let var_id = {
                                let lsm_id = *self.local_semantic_models.read().get(&url).unwrap();
                                let lsm = gsm.get_local_semantic_mut(lsm_id).unwrap();
                                lsm.new_variable(variable)
                            };
                            gsm.private[local_semantic_model_id.0].methods[private_method_id.0]
                                .private_variables
                                .insert(var_name.clone(), var_id);
                            doc.scope_tree.new_symbol(
                                var_name.clone(),
                                SymbolKind::PrivVar,
                                variable_range,
                                Vec::new(),
                                Vec::new(),
                            );
                        }
                    }
                }
            }
        }
    }

    fn add_direct_inherited_class_ids(&self, document: &Document) {
        let mut global_semantic_model = self.global_semantic_model.write();
        let class_def_node = document
            .tree
            .root_node()
            .named_child(document.tree.root_node().named_child_count() - 1)
            .unwrap();
        let children = get_node_children(class_def_node);
        let class_name =
            get_class_name_from_root(document.content.as_str(), document.tree.root_node());
        let class_id = self.classes.read().get(&class_name).unwrap().clone();
        let class = global_semantic_model.classes.get_mut(class_id.0).unwrap();
        class.inherited_classes.clear();
        if children.len() > 3 {
            for node in children[2..].iter() {
                if node.kind() == "class_extends" {
                    let inherited_classes = get_node_children(node.clone());
                    for inherited_class in inherited_classes[1..].iter() {
                        let inherited_class_name =
                            document.content.as_str()[inherited_class.byte_range()].to_string();
                        let inherited_class_id = self
                            .classes
                            .read()
                            .get(&inherited_class_name)
                            .unwrap()
                            .clone();
                        class.inherited_classes.push(inherited_class_id);
                    }
                }
            }
        }
    }

    /// For each class, get it's imported classes classIds and stores them in class.imports
    pub fn add_class_imports(&self, document: &Document) {
        let mut global_semantic_model = self.global_semantic_model.write();
        let children = get_node_children(document.tree.root_node());
        let class_def_node_location = document.tree.root_node().named_child_count() - 1;
        let class_name =
            get_class_name_from_root(document.content.as_str(), document.tree.root_node());
        let class_id = self.classes.read().get(&class_name).unwrap().clone();
        let class = global_semantic_model.classes.get_mut(class_id.0).unwrap();
        for node in children[..class_def_node_location].iter() {
            // these nodes are imports/include/includegen
            if node.kind() == "import_code" {
                let include_clause = node.named_child(1).unwrap();
                let classes = get_node_children(include_clause);
                for imported_class in classes {
                    let imported_class_name =
                        document.content.as_str()[imported_class.byte_range()].to_string();
                    let imported_class_id = self
                        .classes
                        .read()
                        .get(&imported_class_name)
                        .unwrap()
                        .clone();
                    class.imports.push(imported_class_id);
                }
            }
        }
    }
}
