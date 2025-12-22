use crate::common::{get_class_name_from_root, get_node_children};
use crate::document::Document;
use crate::parse_structures::{
    Class, ClassId, FileType, GlobalSemanticModel, LocalSemanticModel,
    LocalSemanticModelId, OverrideIndex,
};
use crate::scope_structures;
use parking_lot::RwLock;
use scope_structures::{GlobalSymbolKind, SymbolId, SymbolKind};
use std::collections::{HashMap};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tower_lsp::lsp_types::Url;

pub struct ProjectState {
    pub(crate) project_root_path: OnceLock<Option<PathBuf>>, //should only ever be set on initialize()
    pub(crate) documents: Arc<RwLock<HashMap<Url, Document>>>,
    pub(crate) global_semantic_model: Arc<RwLock<GlobalSemanticModel>>,
    pub(crate) classes: Arc<RwLock<HashMap<String, ClassId>>>,
    pub(crate) local_semantic_models: Arc<RwLock<HashMap<Url, LocalSemanticModelId>>>,
    pub(crate) defs: Arc<RwLock<HashMap<String, SymbolId>>>,
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
            defs: Arc::new(RwLock::new(HashMap::new())),
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
                        GlobalSymbolKind::Method(method_id),
                        range,
                        url.clone(),
                    );
                    // add method symbol id to project state
                    self.defs.write().insert(method_name, symbol_id);
                } else {
                    // add method to local semantic model
                    let method_id = local_semantic_model.new_method(method);
                    // add methodId to class private methods field
                    class.private_methods.insert(method_name.clone(), method_id);
                    // find current scope and build symbol and add it to the scope
                    let mut docs = self.documents.write();
                    let doc = docs.get_mut(&url).expect("missing doc");
                    let scope_id = doc
                        .scope_tree
                        .find_current_scope(range.start_point)
                        .expect("no scope found");
                    let mut scopes = doc.scope_tree.scopes.write();
                    let mut scope = scopes.get_mut(&scope_id).expect("missing scope");
                    // creates method scope symbol and adds the symbol id to the scope.defs field
                    scope.new_symbol(
                        method_name.clone(),
                        SymbolKind::Method(method_id),
                        range,
                        scope_id,
                    );
                    drop(scopes);
                    drop(docs);
                }
            }
            // add class to global semantic model
            let class_id = global_semantic_model.new_class(class);
            // add class symbol to global semantic model
            // create class global symbol
            let symbol_id = global_semantic_model.new_symbol(
                class_name.clone(),
                GlobalSymbolKind::Class(class_id),
                class_range,
                url.clone(),
            );
            // add class symbol id to project state
            self.defs.write().insert(class_name.clone(), symbol_id);
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

    /// After all documents have been created and the initial build
    /// for classes has completed, this second iteration handles
    /// inheritance and imports.
    pub fn second_iteration(&self) {
        let documents = self.documents.read().values().cloned().collect::<Vec<_>>();
        for document in documents {
            self.add_class_imports(&document);
            self.add_direct_inherited_class_ids(&document);
        }
        let idx = {
            let gsm = self.global_semantic_model.read();
            gsm.build_override_index_public_only()
        };
        *self.override_index.write() = idx;
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
        let mut class = global_semantic_model.classes.get_mut(class_id.0).unwrap();
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
        let mut class = global_semantic_model.classes.get_mut(class_id.0).unwrap();
        for node in children[..class_def_node_location].iter() {
            // these nodes are imports/include/includegen
            if node.kind() == "import_code" {
                let include_clause = node.child(1).unwrap();
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
