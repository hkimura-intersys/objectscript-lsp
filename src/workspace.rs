use crate::common::{get_class_name_from_root, get_node_children, initial_build_scope_tree};
use crate::document::Document;
use crate::method::build_method_calls;
use crate::parse_structures::{Class, ClassId, FileType, LocalSemanticModel, LocalSemanticModelId, MethodCallSite, OverrideIndex, PrivateMethodId, PublicMethodId};
use crate::scope_structures;
use parking_lot::{Mutex, RwLock};
use scope_structures::{ClassGlobalSymbolId, MethodGlobalSymbolId, VariableGlobalSymbolId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tower_lsp::lsp_types::Url;
use tree_sitter::{Parser, StreamingIterator, Tree, Node};
use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};
use crate::global_semantic::GlobalSemanticModel;

pub struct WorkspaceParsers {
    pub(crate) routine: Mutex<Parser>,
    pub(crate) cls: Mutex<Parser>,
}

impl WorkspaceParsers {
    pub fn new() -> Self {
        let mut cls_parser = Parser::new();
        cls_parser
            .set_language(&LANGUAGE_OBJECTSCRIPT.into())
            .expect("Error loading ObjectScript grammar");

        let mut routine_parser = Parser::new();
        routine_parser
            .set_language(&LANGUAGE_OBJECTSCRIPT_CORE.into())
            .expect("Error loading Core ObjectScript grammar");

        Self {
            routine: Mutex::new(routine_parser),
            cls: Mutex::new(cls_parser),
        }
    }
}
pub struct ProjectState {
    pub(crate) project_root_path: OnceLock<Option<PathBuf>>, //should only ever be set on initialize()
    pub(crate) documents: Arc<RwLock<HashMap<Url, Document>>>,
    pub(crate) global_semantic_model: Arc<RwLock<GlobalSemanticModel>>,
    pub(crate) classes: Arc<RwLock<HashMap<String, ClassId>>>,
    // pub(crate) local_semantic_models: Arc<RwLock<HashMap<Url, LocalSemanticModelId>>>,
    pub(crate) class_defs: Arc<RwLock<HashMap<String, ClassGlobalSymbolId>>>,
    pub(crate) pub_method_defs: Arc<RwLock<HashMap<String, HashMap<String, MethodGlobalSymbolId>>>>,
    // Var name -> Hash <Class name -> Vec: var defs>
    pub(crate) pub_var_defs: Arc<RwLock<HashMap<String, HashMap<String, Vec<VariableGlobalSymbolId>>>>>,
    pub(crate) override_index: Arc<RwLock<OverrideIndex>>,
    pub(crate) parsers: WorkspaceParsers
}

impl ProjectState {
    pub fn new() -> Self {
        Self {
            project_root_path: OnceLock::new(),
            documents: Arc::new(RwLock::new(HashMap::new())),
            global_semantic_model: Arc::new(RwLock::new(GlobalSemanticModel::new())),
            classes: Arc::new(RwLock::new(HashMap::new())),
            class_defs: Arc::new(RwLock::new(HashMap::new())),
            pub_method_defs: Arc::new(RwLock::new(HashMap::new())),
            pub_var_defs: Arc::new(RwLock::new(HashMap::new())),
            override_index: Arc::new(RwLock::new(OverrideIndex::new())),
            parsers: WorkspaceParsers::new(),
        }
    }

    pub fn handle_document_opened(&self, url: Url, text: String, file_type: FileType, version: i32) {
        let documents = self.documents.read();
        let curr_document = documents.get(&url);
        if curr_document.is_none() {
            drop(documents);
            let new_tree = if file_type == FileType::Cls {
                self.parsers.cls.lock().parse(&text, None).unwrap()
            } else {
                self.parsers.routine.lock().parse(&text, None).unwrap()
            };
            if file_type == FileType::Cls {
                let class_name = get_class_name_from_root(&text, new_tree.root_node().clone());
                let document = Document::new(text.clone(), new_tree.clone(), file_type.clone(), class_name);
                self.add_document(url,document);
            }
            else {
                let document = Document::new(text.clone(), new_tree.clone(), file_type.clone(), "TODO".to_string());
                self.add_routine_document(url,document);
            }
        }
        else {
            let curr_document_content = curr_document.unwrap().content.clone();
            let curr_document_file_type = curr_document.unwrap().file_type.clone();
            let curr_version = if curr_document.unwrap().version.is_none() {
                -1
            } else {
                curr_document.unwrap().version.unwrap()
            };
            drop(documents);
            if curr_document_content.as_str() != text.as_str() || curr_document_file_type != file_type {
                let new_tree = if file_type == FileType::Cls {
                    self.parsers.cls.lock().parse(&text, None).unwrap()
                } else {
                    self.parsers.routine.lock().parse(&text, None).unwrap()
                };
                self.update_document(url, new_tree, file_type, version, text.as_str());
            }
            else {
                if curr_version == -1 || version != curr_version {
                    self.update_document_version(url, version);
                }
            }
        }
    }

    pub fn get_document_info(&self, url: &Url) -> (FileType, String, i32, Tree) {
        let documents = self.documents.read();
        let curr_document = documents.get(url).unwrap();
        let curr_version = curr_document.version.unwrap_or(0).clone();
        let current_text = curr_document.content.clone();
        let curr_tree = curr_document.tree.clone();
        (curr_document.file_type.clone(), current_text, curr_version, curr_tree)
    }

    pub fn rebuild_semantics(&self, url: Url, node: Node, content: &str, class_id: ClassId, class_symbol_id: ClassGlobalSymbolId, local_semantic_model_id: LocalSemanticModelId, class_name: String, old_class_name: String, file_type: FileType) {
        if file_type != FileType::Cls {
            return;
        }
        // build vec of public methods to add to gsm at the end
        let mut gsm_methods = Vec::new();
        let mut lsm_methods = Vec::new();
        // Create a new class, will reassign the class at class_id to this new class.
        let mut class = Class::new(class_name.clone());
        let methods = class.initial_build(node, content);
        let mut global_semantic_model = self.global_semantic_model.write();
        global_semantic_model.update_class_symbol(class_name.clone(), node.range(), url.clone(), class_symbol_id);
        // class id dne yet, because it gets added after. instead, we can just create the method ids here
        for (method, range) in methods {
            let method_name = method.name.clone();
            if method.is_public {
                // add method to global semantic model
                let method_id = PublicMethodId(gsm_methods.len());
                gsm_methods.push(method);
                let method_symbol_id = global_semantic_model.new_method_symbol(method_name.clone(), range, url.clone(), class_symbol_id);
                // add method symbol
                self.pub_method_defs.write().entry(class_name.clone()).or_insert_with(HashMap::new).insert(method_name.clone(), method_symbol_id);
                // add methodId to class public methods field
                class.public_methods.insert(method_name.clone(), method_id);
            } else {
                // add method to local semantic model
                let method_id = PrivateMethodId(lsm_methods.len());
                lsm_methods.push(method);
                // add methodId to class private methods field
                class.private_methods.insert(method_name.clone(), method_id);
                // find current scope and build symbol and add it to the scope
                let mut docs = self.documents.write();
                let doc = docs.get_mut(&url).expect("missing doc");
                // this creates the symbol and adds the symbol id to the scope tree
                doc.scope_tree.new_method_symbol(method_name.clone(), range);
                drop(docs);
            }
        }

        global_semantic_model.classes[class_id.0] = class;
        for method in gsm_methods {
            global_semantic_model.new_method(method, class_id);
        }

        let local_semantic_model = global_semantic_model.get_local_semantic_mut(local_semantic_model_id).unwrap();
        for method in lsm_methods {
            local_semantic_model.new_method(method);
        }
        local_semantic_model.active = true;
        drop(global_semantic_model);

        // remove old class name, just in case the name changed
        self.classes.write().remove(&old_class_name);
        // add class id corresponding to class struct
        self.classes.write().insert(class_name.clone(), class_id);
        // let local_semantic_id = global_semantic_model.new_local_semantic(local_semantic_model);
        let mut docs = self.documents.write();
        let doc = docs.get_mut(&url).expect("missing doc");
        // this creates the symbol and adds the symbol id to the scope tree
        doc.local_semantic_model_id = Some(local_semantic_model_id);
        doc.class_id = Some(class_id);
        drop(docs);
    }

    pub fn add_document(&self, url: Url, document: Document) {
        if matches!(document.file_type.clone(), FileType::Cls) {
            let class_name = document.class_name.clone();
            // create class struct
            let mut class = Class::new(class_name.clone());
            let mut local_semantic_model = LocalSemanticModel::new();

            // build vec of public methods to add to gsm at the end
            let mut gsm_methods = Vec::new();

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

            let class_symbol_id = global_semantic_model.new_class_symbol(class_name.clone(), class_range, url.clone());
            // add to scope tree
            self.documents.write().get_mut(&url).unwrap().scope_tree.class_def = Some(class_symbol_id);
            self.class_defs
                .write()
                .insert(class_name.clone(), class_symbol_id);

                // class id dne yet, because it gets added after. instead, we can just create the method ids here
            for (method, range) in methods {
                let method_name = method.name.clone();
                if method.is_public {
                    // add method to global semantic model
                    let method_id = PublicMethodId(gsm_methods.len());
                    gsm_methods.push(method);
                    // add methodId to class public methods field
                    class.public_methods.insert(method_name.clone(), method_id);

                    // creates method global symbol in global semantic model
                    let method_symbol_id = global_semantic_model.new_method_symbol(method_name.clone(), range, url.clone(), class_symbol_id);
                    // add method symbol
                    self.pub_method_defs.write().entry(class_name.clone()).or_insert_with(HashMap::new).insert(method_name.clone(), method_symbol_id);
                } else {
                    // add method to local semantic model
                    let method_id = local_semantic_model.new_method(method);
                    // add methodId to class private methods field
                    class.private_methods.insert(method_name.clone(), method_id);
                    // find current scope and build symbol and add it to the scope
                    let mut docs = self.documents.write();
                    let doc = docs.get_mut(&url).expect("missing doc");
                    // this creates the symbol and adds the symbol id to the scope tree
                    doc.scope_tree.new_method_symbol(method_name.clone(), range);
                    drop(docs);
                }
            }
            // add class to global semantic model
            let class_id = global_semantic_model.new_class(class);
            for method in gsm_methods {
                global_semantic_model.new_method(method, class_id);
            }

            // add class id corresponding to class struct
            self.classes.write().insert(class_name.clone(), class_id);

            let local_semantic_id = global_semantic_model.new_local_semantic(local_semantic_model);
            let mut docs = self.documents.write();
            let doc = docs.get_mut(&url).expect("missing doc");
            // this creates the symbol and adds the symbol id to the scope tree
            doc.local_semantic_model_id = Some(local_semantic_id);
            doc.class_id = Some(class_id);
            drop(global_semantic_model);
            drop(docs);
        }
    }

    pub fn update_document(&self, url: Url, tree: Tree, file_type: FileType, version: i32, content: &str) {
        // get the class symbol, the class id (matches class struct), and the local semantic model id
        let class_name = get_class_name_from_root(content, tree.root_node());
        let (class_symbol_id, class_id, local_semantic_id, old_class_name) = {
            let documents = self.documents.read();
            let document =  documents.get(&url).unwrap();
            (document.scope_tree.class_def.unwrap(), document.class_id.unwrap(), document.local_semantic_model_id.unwrap(), document.class_name.clone())
        };
        let mut scope_tree = initial_build_scope_tree(tree.clone());
        scope_tree.class_def = Some(class_symbol_id);
        self.documents.write().get_mut(&url).expect("missing doc").scope_tree = scope_tree;

        // clear everything
        let mut global_semantic_model = self.global_semantic_model.write();
        global_semantic_model.remove_document_symbols(class_symbol_id);
        global_semantic_model.reset_doc_semantics(class_id, class_name.clone(), local_semantic_id);
        drop(global_semantic_model);

        let node = tree
            .root_node()
            .named_child(tree.root_node().named_child_count() - 1)
            .unwrap();

        // remove anything related to the class from before
        // TODO: how do i remove the class references from other classes... (I guess this means I have to reparse more than just one doc)
        self.override_index.write().effective_public_methods.remove(&class_id);

        // rebuild semantics
        self.rebuild_semantics(url.clone(), node, content, class_id, class_symbol_id, local_semantic_id, class_name.clone(), old_class_name, file_type.clone());

        // update document
        let mut documents = self.documents.write();
        let document = documents.get_mut(&url).unwrap();
        document.version = Some(version);
        document.file_type = file_type;
        document.tree = tree;
        document.content = content.to_string();
        document.class_name = class_name;
        drop(documents);
    }

    pub fn update_document_version(&self, url: Url, version: i32) {
        let mut documents = self.documents.write();
        let document = documents.get_mut(&url).unwrap();
        document.version = Some(version);
    }

    pub fn add_routine_document(&self, url: Url, document: Document) {
        // TODO
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
            /*
            The class symbol id and class id are referenced later on in this function. They are how we know which class corresponds to the methods and variables.
             */
            let class_symbol_id = self.class_defs.read().get(&class_name).expect("missing class symbol").clone();
            let class_id = self.classes.read().get(&class_name).expect("missing class struct").clone();
            let url = gsm.class_defs[class_symbol_id.0].url.clone();
            let doc = docs.get_mut(&url).expect("missing doc");
            let content = doc.content.as_str();
            let tree_root_node = doc.tree.root_node();
            let scope_tree = doc.scope_tree.clone();
            let local_semantic_id = doc.local_semantic_model_id.unwrap().clone();

            for pub_method_id in public_method_ids {
                let (method_name, loc) = {
                    let class_id = self.classes.read().get(&class_name).expect("missing class id").clone();
                    let method_name = &gsm.methods.get(&class_id).unwrap()[pub_method_id.0].name;
                    let sym_id = self.pub_method_defs.read().get(&class_name).unwrap().get(method_name).unwrap().clone();
                    let sym = &gsm.method_defs.get(&class_symbol_id).unwrap()[sym_id.0];
                    (method_name.clone(), sym.location)
                };
                let method_definition_node = tree_root_node
                    .named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                    .unwrap();
                let calls = build_method_calls(&class_name, method_definition_node, content);
                let new_sites: Vec<MethodCallSite> = calls
                    .into_iter()
                    .map(|call| {
                        let callee_symbol = self.pub_method_defs.read().get(&call.callee_class.clone()).unwrap().get(&call.callee_method).copied();
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
                    let method = &gsm.methods.get(&class_id).unwrap()[pub_method_id.0];
                    method.build_method_variables_and_ref(method_definition_node, content)
                };
                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in result {
                    let var_name = variable.name.clone();
                    if variable.is_public {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_id = gsm.new_variable(variable, &class_id);
                            gsm.methods.get_mut(&class_id).unwrap()[pub_method_id.0]
                                .public_variables
                                .insert(var_name.clone(), var_id);
                            let symbol_id = gsm.new_variable_symbol(
                                var_name.clone(),
                                variable_range,
                                url.clone(),
                                refs_to_other_vars.clone(),
                                refs_to_properties.clone(),
                                class_symbol_id.clone(),
                            );
                            doc.scope_tree.new_public_var_symbol(
                                var_name.clone(),
                                variable_range,
                                symbol_id,
                            );
                            self.pub_var_defs
                                .write()
                                .entry(var_name.clone())
                                .or_insert(HashMap::new()).entry(class_name.clone()).or_insert(Vec::new()).push(symbol_id);
                        }
                    } else {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_name = variable.name.clone();
                            let var_id = {
                                let lsm = gsm.get_local_semantic_mut(local_semantic_id).unwrap();
                                lsm.new_variable(variable)
                            };
                            gsm.methods.get_mut(&class_id).unwrap()[pub_method_id.0]
                                .private_variables
                                .insert(var_name.clone(), var_id);
                            doc.scope_tree.new_variable_symbol(
                                var_name.clone(),
                                variable_range,
                                refs_to_other_vars.clone(),
                                refs_to_properties.clone(),
                            );
                        }
                    }
                }
            }

            for private_method_id in private_method_ids {
                let (method_name, loc) = {
                    let m = &gsm.private[local_semantic_id.0].methods[private_method_id.0];
                    // I need to get the sym id from the scope tree
                    let (scope_id, sym_id) = scope_tree
                        .get_private_method_symbol(&m.name)
                        .expect("missing private method symbol");
                    let sym = scope_tree
                        .scopes
                        .read()
                        .get(&scope_id)
                        .expect("missing scope")
                        .method_symbols[sym_id.0]
                        .clone();
                    (m.name.clone(), sym.location)
                };
                let method_definition_node = tree_root_node
                    .named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                    .unwrap();
                let calls = build_method_calls(&class_name, method_definition_node, content);
                let new_sites: Vec<MethodCallSite> = calls
                    .into_iter()
                    .map(|call| {
                        let callee_symbol = self.pub_method_defs.read().get(&call.callee_class.clone()).unwrap().get(&call.callee_method).copied();
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
                    let method = &gsm.private[local_semantic_id.0].methods[private_method_id.0];
                    method.build_method_variables_and_ref(method_definition_node, content)
                };
                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in result {
                    let var_name = variable.name.clone();
                    if variable.is_public {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_id = gsm.new_variable(variable, &class_id);
                            gsm.private[local_semantic_id.0].methods[private_method_id.0]
                                .public_variables
                                .insert(var_name.clone(), var_id);
                            let symbol_id = gsm.new_variable_symbol(
                                var_name.clone(),
                                variable_range,
                                url.clone(),
                                refs_to_other_vars.clone(),
                                refs_to_properties.clone(),
                                class_symbol_id.clone(),
                            );
                            doc.scope_tree.new_public_var_symbol(
                                var_name.clone(),
                                variable_range,
                                symbol_id,
                            );
                            self.pub_var_defs
                                .write()
                                .entry(var_name.clone())
                                .or_insert(HashMap::new()).entry(class_name.clone()).or_insert(Vec::new()).push(symbol_id);
                        }
                    } else {
                        if !refs_to_other_vars.contains(&var_name) {
                            let var_name = variable.name.clone();
                            let var_id = {
                                let lsm = gsm.get_local_semantic_mut(local_semantic_id).unwrap();
                                lsm.new_variable(variable)
                            };
                            gsm.private[local_semantic_id.0].methods[private_method_id.0]
                                .private_variables
                                .insert(var_name.clone(), var_id);
                            doc.scope_tree.new_variable_symbol(
                                var_name.clone(),
                                variable_range,
                                refs_to_other_vars.clone(),
                                refs_to_properties.clone(),
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
        class.imports.clear();
        for node in children[..class_def_node_location].iter() {
            // these nodes are imports/includ
            // /includegen
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
