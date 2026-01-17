use crate::common::{get_class_name_from_root, get_node_children, initial_build_scope_tree};
use crate::config::Config;
use crate::document::Document;
use crate::global_semantic::GlobalSemanticModel;
use crate::method::build_method_calls;
use crate::override_index::OverrideIndex;
use crate::parse_structures::{
    Class, ClassId, FileType, LocalSemanticModel, LocalSemanticModelId, MethodCallSite,
    PrivateMethodId, PublicMethodId, PublicMethodRef,
};
use crate::scope_structures;
use parking_lot::{Mutex, RwLock};
use scope_structures::{ClassGlobalSymbolId, MethodGlobalSymbolId, VariableGlobalSymbolId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tower_lsp::lsp_types::Url;
use tree_sitter::{Node, Parser, Range, Tree, Point};
use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};

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

pub struct ProjectData {
    pub(crate) config: Config,
    pub(crate) documents: HashMap<Url, Document>,
    pub(crate) global_semantic_model: GlobalSemanticModel,
    pub(crate) classes: HashMap<String, ClassId>,
    pub(crate) class_defs: HashMap<String, ClassGlobalSymbolId>,
    pub(crate) pub_method_defs: HashMap<String, HashMap<String, MethodGlobalSymbolId>>,
    // variable name -> map<class name -> symbol locations>
    pub(crate) pub_var_defs: HashMap<String, HashMap<String, Vec<VariableGlobalSymbolId>>>,
    pub(crate) override_index: OverrideIndex,
}

pub struct ProjectState {
    pub(crate) project_root_path: OnceLock<Option<PathBuf>>, //should only ever be set on initialize()
    pub(crate) data: RwLock<ProjectData>,
    pub(crate) parsers: WorkspaceParsers,
}

impl ProjectData {
    pub fn get_document_info(&self, url: &Url) -> Option<(FileType, String, i32, Tree)> {
        let curr_document = self.documents.get(url)?;
        let curr_version = curr_document.version.unwrap_or(0);
        let current_text = curr_document.content.clone();
        let curr_tree = curr_document.tree.clone();
        Some((
            curr_document.file_type.clone(),
            current_text,
            curr_version,
            curr_tree,
        ))
    }

    pub fn add_document_if_absent(
        &mut self,
        url: Url,
        code: String,
        tree: Tree,
        filetype: FileType,
        class_name: String,
        version: Option<i32>,
    ) {
        if self.documents.contains_key(&url) {
            return; // IMPORTANT: don't overwrite editor state
        }
        self.add_document(url, code, tree, filetype, class_name, version);
    }
    pub fn add_document(
        &mut self,
        url: Url,
        code: String,
        tree: Tree,
        filetype: FileType,
        class_name: String,
        version: Option<i32>,
    ) {
        if matches!(filetype.clone(), FileType::Cls) {
            // get class def node
            let Some(node) = tree
                .root_node()
                .named_child(tree.root_node().named_child_count() - 1)
            else {
                return;
            };
            let class_range = node.range();
            let content = code.as_str();
            // create class struct
            let mut local_semantic_model = LocalSemanticModel::new();
            // build vec of public methods to add to gsm at the end
            let mut gsm_methods = Vec::new();
            let mut class = Class::new(class_name.clone());
            let methods = class.initial_build(node, content);
            let class_symbol_id = self.global_semantic_model.new_class_symbol(
                class_name.clone(),
                class_range,
                url.clone(),
            );
            let scope_tree = initial_build_scope_tree(tree.clone(), class_symbol_id);
            let mut document = Document::new(
                code,
                tree,
                filetype,
                class_name.clone(),
                scope_tree,
                version,
            );

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
                    let method_symbol_id = self.global_semantic_model.new_method_symbol(
                        method_name.clone(),
                        range,
                        url.clone(),
                        class_symbol_id,
                    );
                    // add method symbol
                    self.pub_method_defs
                        .entry(class_name.clone())
                        .or_insert_with(HashMap::new)
                        .insert(method_name.clone(), method_symbol_id);
                } else {
                    // add method to local semantic model
                    let method_id = local_semantic_model.new_method(method);
                    // add methodId to class private methods field
                    class.private_methods.insert(method_name.clone(), method_id);
                    // find current scope and build symbol and add it to the scope
                    // this creates the symbol and adds the symbol id to the scope tree
                    document
                        .scope_tree
                        .new_method_symbol(method_name.clone(), range);
                }
            }
            // add class to global semantic model
            let class_id = self.global_semantic_model.new_class(class);
            for method in gsm_methods {
                self.global_semantic_model.new_method(method, class_id);
            }

            // add class id corresponding to class struct
            self.classes.insert(class_name.clone(), class_id);

            let local_semantic_id = self
                .global_semantic_model
                .new_local_semantic(local_semantic_model);
            // this creates the symbol and adds the symbol id to the scope tree
            document.local_semantic_model_id = Some(local_semantic_id);
            document.class_id = Some(class_id);
            self.documents.insert(url.clone(), document);
            self.class_defs.insert(class_name.clone(), class_symbol_id);
        }
    }

    pub fn update_document(
        &mut self,
        url: Url,
        tree: Tree,
        file_type: FileType,
        version: i32,
        content: &str,
    ) {
        let class_name = get_class_name_from_root(content, tree.root_node());

        let (class_symbol_id, class_id, local_semantic_id, old_class_name) = {
            let Some(doc) = self.documents.get(&url) else {
                eprintln!("Warning: No document found for url: {}", url);
                return;
            };
            let Some(class_id) = doc.class_id else {
                eprintln!("Warning: No class id found in document for url: {}", url);
                return;
            };

            let Some(local_semantic_model_id) = doc.local_semantic_model_id else {
                eprintln!(
                    "Warning: No local_semantic_model_id found in document for url: {}",
                    url
                );
                return;
            };
            (
                doc.scope_tree.class_def,
                class_id,
                local_semantic_model_id,
                doc.class_name.clone(),
            )
        };

        // Update scope tree in a short mutable borrow
        {
            let Some(doc) = self.documents.get_mut(&url) else {
                eprintln!("Warning: No document found for url: {}", url);
                return;
            };
            doc.scope_tree = initial_build_scope_tree(tree.clone(), class_symbol_id);
        }

        // Clear semantics
        self.global_semantic_model
            .remove_document_symbols(class_symbol_id);
        self.global_semantic_model.reset_doc_semantics(
            class_id,
            class_name.clone(),
            local_semantic_id,
        );

        if old_class_name != class_name {
            self.class_defs.remove(&old_class_name);
            self.pub_method_defs.remove(&old_class_name);
            self.classes.remove(&old_class_name);
        }

        let Some(node) = tree
            .root_node()
            .named_child(tree.root_node().named_child_count() - 1)
        else {
            return;
        };

        // Rebuild semantics (no document borrow alive here)
        self.rebuild_semantics(
            url.clone(),
            node,
            content,
            class_id,
            class_symbol_id,
            local_semantic_id,
            class_name.clone(),
            old_class_name,
            file_type.clone(),
        );

        // Update document fields in another short mutable borrow
        {
            let Some(doc) = self.documents.get_mut(&url) else {
                eprintln!("Warning: No document found for url: {}", url);
                return;
            };
            doc.version = Some(version);
            doc.file_type = file_type;
            doc.tree = tree;
            doc.content = content.to_string();
            doc.class_name = class_name;
        }

        // Recompute inheritance/override/calls/vars
        self.build_inheritance_and_variables(Some(url));
    }

    pub fn rebuild_semantics(
        &mut self,
        url: Url,
        node: Node,
        content: &str,
        class_id: ClassId,
        class_symbol_id: ClassGlobalSymbolId,
        local_semantic_model_id: LocalSemanticModelId,
        class_name: String,
        old_class_name: String,
        file_type: FileType,
    ) {
        if file_type != FileType::Cls {
            return;
        }
        // build vec of public methods to add to gsm at the end
        let mut gsm_methods = Vec::new();
        let mut lsm_methods = Vec::new();
        // Create a new class, will reassign the class at class_id to this new class.
        let mut class = Class::new(class_name.clone());
        let methods = class.initial_build(node, content);
        self.global_semantic_model.update_class_symbol(
            class_name.clone(),
            node.range(),
            url.clone(),
            class_symbol_id,
        );
        // class id dne yet, because it gets added after. instead, we can just create the method ids here
        for (method, range) in methods {
            let method_name = method.name.clone();
            if method.is_public {
                // add method to global semantic model
                let method_id = PublicMethodId(gsm_methods.len());
                gsm_methods.push(method);
                let method_symbol_id = self.global_semantic_model.new_method_symbol(
                    method_name.clone(),
                    range,
                    url.clone(),
                    class_symbol_id,
                );
                // add method symbol
                self.pub_method_defs
                    .entry(class_name.clone())
                    .or_insert_with(HashMap::new)
                    .insert(method_name.clone(), method_symbol_id);
                // add methodId to class public methods field
                class.public_methods.insert(method_name.clone(), method_id);
            } else {
                // add method to local semantic model
                let method_id = PrivateMethodId(lsm_methods.len());
                lsm_methods.push(method);
                // add methodId to class private methods field
                class.private_methods.insert(method_name.clone(), method_id);
                // find current scope and build symbol and add it to the scope
                let Some(doc) = self.documents.get_mut(&url) else {
                    eprintln!("Warning: No document found for url: {}", url);
                    return;
                };
                // this creates the symbol and adds the symbol id to the scope tree
                doc.scope_tree.new_method_symbol(method_name.clone(), range);
            }
        }

        self.global_semantic_model.classes[class_id.0] = class;
        for method in gsm_methods {
            self.global_semantic_model.new_method(method, class_id);
        }

        let Some(local_semantic_model) = self
            .global_semantic_model
            .get_local_semantic_mut(local_semantic_model_id)
        else {
            return;
        };
        for method in lsm_methods {
            local_semantic_model.new_method(method);
        }
        local_semantic_model.active = true;
        // remove old class name, just in case the name changed
        self.classes.remove(&old_class_name);
        // add class id corresponding to class struct
        self.classes.insert(class_name.clone(), class_id);
        // let local_semantic_id = global_semantic_model.new_local_semantic(local_semantic_model);
        let Some(doc) = self.documents.get_mut(&url) else {
            return;
        };
        // this creates the symbol and adds the symbol id to the scope tree
        doc.local_semantic_model_id = Some(local_semantic_model_id);
        doc.class_id = Some(class_id);
    }

    pub fn build_inheritance_and_variables(&mut self, only: Option<Url>) {
        // Which documents should update imports/extends?
        let urls: Vec<Url> = match only {
            Some(u) => vec![u],
            None => self.documents.keys().cloned().collect(),
        };

        for url in &urls {
            self.recompute_imports_for_url(url);
            self.recompute_extends_for_url(url);
        }

        // Recompute inheritance + override index
        self.global_semantic_model.class_keyword_inheritance();
        self.override_index = self.global_semantic_model.build_override_index();

        // (Highly recommended) clear old callsites so repeated builds don't duplicate
        for c in &mut self.global_semantic_model.classes {
            c.method_calls.clear();
        }

        let idx = &self.override_index;
        let classes_map = &self.classes;

        for i in 0..self.global_semantic_model.classes.len() {
            let (class_name, public_method_ids, private_method_ids) = {
                let class = &self.global_semantic_model.classes[i];
                (
                    class.name.clone(),
                    class.public_methods.values().cloned().collect::<Vec<_>>(),
                    class.private_methods.values().cloned().collect::<Vec<_>>(),
                )
            };

            let class_symbol_id = match self.class_defs.get(&class_name).copied() {
                Some(id) => id,
                None => continue,
            };
            let class_id = match self.classes.get(&class_name).copied() {
                Some(id) => id,
                None => continue,
            };

            let url = self.global_semantic_model.class_defs[class_symbol_id.0]
                .url
                .clone();

            // We need to mutate scope_tree with var symbols, so get_mut
            let doc = match self.documents.get_mut(&url) {
                Some(d) => d,
                None => continue,
            };

            let content = doc.content.as_str();
            let tree_root_node = doc.tree.root_node();
            let scope_tree_snapshot = doc.scope_tree.clone(); // for private method symbol lookup
            let local_semantic_id = match doc.local_semantic_model_id {
                Some(id) => id,
                None => continue,
            };

            // ---------- public methods ----------
            for pub_method_id in public_method_ids {
                let (method_name, loc) = {
                    let Some(methods) = self.global_semantic_model.methods.get(&class_id) else {
                        eprintln!("Failed to get methods for class id {:?}", class_id);
                        continue;
                    };
                    let Some(method) = methods.get(pub_method_id.0) else {
                        eprintln!("Failed to get method  for class id {:?}", class_id);
                        continue;
                    };

                    let method_name = method.name.clone();

                    let Some(&sym_id) = self
                        .pub_method_defs
                        .get(&class_name)
                        .and_then(|m| m.get(&method_name))
                    else {
                        eprintln!(
                            "Failed to get public method symbol id for class id {:?}",
                            class_id
                        );
                        continue;
                    };

                    let Some(method_symbols) =
                        self.global_semantic_model.method_defs.get(&class_symbol_id)
                    else {
                        eprintln!("Failed to get method symbols for class id {:?}", class_id);
                        continue;
                    };

                    let Some(sym) = method_symbols.get(sym_id.0) else {
                        eprintln!("Failed to get method symbol for sym id {:?}", sym_id);
                        continue;
                    };

                    (method_name, sym.location)
                };

                let Some(method_definition_node) =
                    tree_root_node.named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                else {
                    continue;
                };

                // Calls
                let calls = build_method_calls(&class_name, method_definition_node, content);

                let new_sites: Vec<MethodCallSite> = calls
                    .into_iter()
                    .map(|call| {
                        let callee_symbol = classes_map
                            .get(&call.callee_class)
                            .copied()
                            .and_then(|callee_class_id| {
                                idx.effective_public_methods.get(&callee_class_id)
                            })
                            .and_then(|tbl| tbl.get(&call.callee_method).copied());

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

                self.global_semantic_model.classes[i]
                    .method_calls
                    .extend(new_sites);

                // Variables: compute first (immutable), then apply (mutable) to avoid long borrows
                let var_results = {
                    let method = &self
                        .global_semantic_model
                        .methods
                        .get(&class_id)
                        .expect("missing methods vec")[pub_method_id.0];
                    method.build_method_variables_and_ref(method_definition_node, content)
                };

                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in
                    var_results
                {
                    let var_name = variable.name.clone();

                    if refs_to_other_vars.contains(&var_name) {
                        continue;
                    }

                    if variable.is_public {
                        let var_id = self.global_semantic_model.new_variable(variable, &class_id);

                        {
                            let Some(m) = self
                                .global_semantic_model
                                .methods
                                .get_mut(&class_id)
                                .and_then(|v| v.get_mut(pub_method_id.0))
                            else {
                                continue; // skip this method
                            };
                            m.public_variables.insert(var_name.clone(), var_id);
                        }

                        let symbol_id = self.global_semantic_model.new_variable_symbol(
                            var_name.clone(),
                            variable_range,
                            url.clone(),
                            refs_to_other_vars.clone(),
                            refs_to_properties.clone(),
                            class_symbol_id,
                        );

                        doc.scope_tree.new_public_var_symbol(
                            var_name.clone(),
                            variable_range,
                            symbol_id,
                        );

                        self.pub_var_defs
                            .entry(var_name)
                            .or_insert_with(HashMap::new)
                            .entry(class_name.clone())
                            .or_insert_with(Vec::new)
                            .push(symbol_id);
                    } else {
                        let var_id = {
                            let Some(lsm) = self
                                .global_semantic_model
                                .get_local_semantic_mut(local_semantic_id)
                            else {
                                continue;
                            };
                            lsm.new_variable(variable)
                        };

                        {
                            let Some(m) = self
                                .global_semantic_model
                                .methods
                                .get_mut(&class_id)
                                .and_then(|v| v.get_mut(pub_method_id.0))
                            else {
                                continue; // skip this method
                            };
                            m.private_variables.insert(var_name.clone(), var_id);
                        }

                        doc.scope_tree.new_variable_symbol(
                            var_name,
                            variable_range,
                            refs_to_other_vars,
                            refs_to_properties,
                        );
                    }
                }
            }

            // ---------- private methods ----------
            for private_method_id in private_method_ids {
                let (method_name, loc) = {
                    let Some(lsm) = self
                        .global_semantic_model
                        .get_local_semantic_mut(local_semantic_id)
                    else {
                        continue;
                    };
                    let Some(m) = lsm.methods.get(private_method_id.0) else {
                        continue;
                    };

                    let Some((scope_id, sym_id)) =
                        scope_tree_snapshot.get_private_method_symbol_id(&m.name)
                    else {
                        continue;
                    };

                    let sym = match scope_tree_snapshot
                        .scopes
                        .get(&scope_id)
                        .and_then(|scope| scope.method_symbols.get(sym_id.0))
                    {
                        Some(sym) => sym.clone(),
                        None => continue,
                    };

                    (m.name.clone(), sym.location)
                };

                let Some(method_definition_node) =
                    tree_root_node.named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                else {
                    continue;
                };

                let calls = build_method_calls(&class_name, method_definition_node, content);

                let new_sites: Vec<MethodCallSite> = calls
                    .into_iter()
                    .map(|call| {
                        let callee_symbol = classes_map
                            .get(&call.callee_class)
                            .copied()
                            .and_then(|callee_class_id| {
                                idx.effective_public_methods.get(&callee_class_id)
                            })
                            .and_then(|tbl| tbl.get(&call.callee_method).copied());

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

                self.global_semantic_model.classes[i]
                    .method_calls
                    .extend(new_sites);

                let var_results = {
                    let method = &self.global_semantic_model.private[local_semantic_id.0].methods
                        [private_method_id.0];
                    method.build_method_variables_and_ref(method_definition_node, content)
                };

                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in
                    var_results
                {
                    let var_name = variable.name.clone();

                    if refs_to_other_vars.contains(&var_name) {
                        continue;
                    }

                    if variable.is_public {
                        let var_id = self.global_semantic_model.new_variable(variable, &class_id);

                        {
                            let m = &mut self.global_semantic_model.private[local_semantic_id.0]
                                .methods[private_method_id.0];
                            m.public_variables.insert(var_name.clone(), var_id);
                        }

                        let symbol_id = self.global_semantic_model.new_variable_symbol(
                            var_name.clone(),
                            variable_range,
                            url.clone(),
                            refs_to_other_vars.clone(),
                            refs_to_properties.clone(),
                            class_symbol_id,
                        );

                        doc.scope_tree.new_public_var_symbol(
                            var_name.clone(),
                            variable_range,
                            symbol_id,
                        );

                        self.pub_var_defs
                            .entry(var_name)
                            .or_insert_with(HashMap::new)
                            .entry(class_name.clone())
                            .or_insert_with(Vec::new)
                            .push(symbol_id);
                    } else {
                        let var_id = {
                            let Some(lsm) = self
                                .global_semantic_model
                                .get_local_semantic_mut(local_semantic_id)
                            else {
                                continue;
                            };
                            lsm.new_variable(variable)
                        };

                        {
                            let m = &mut self.global_semantic_model.private[local_semantic_id.0]
                                .methods[private_method_id.0];
                            m.private_variables.insert(var_name.clone(), var_id);
                        }

                        doc.scope_tree.new_variable_symbol(
                            var_name,
                            variable_range,
                            refs_to_other_vars,
                            refs_to_properties,
                        );
                    }
                }
            }
        }
    }

    fn recompute_imports_for_url(&mut self, url: &Url) {
        let (tree, content, class_name) = match self.documents.get(url) {
            Some(d) => (d.tree.clone(), d.content.clone(), d.class_name.clone()),
            None => return,
        };

        let class_id = match self.classes.get(&class_name).copied() {
            Some(id) => id,
            None => return,
        };

        let children = get_node_children(tree.root_node());
        let class_def_node_location = tree.root_node().named_child_count() - 1;

        let mut imports = Vec::new();
        for node in children[..class_def_node_location].iter() {
            if node.kind() == "import_code" {
                if let Some(include_clause) = node.named_child(1) {
                    for imported_class in get_node_children(include_clause) {
                        let imported_name =
                            content.as_str()[imported_class.byte_range()].to_string();
                        if let Some(id) = self.classes.get(&imported_name).copied() {
                            imports.push(id);
                        }
                    }
                }
            }
        }

        if let Some(class) = self.global_semantic_model.classes.get_mut(class_id.0) {
            class.imports = imports;
        }
    }

    fn recompute_extends_for_url(&mut self, url: &Url) {
        let (tree, content, class_name) = match self.documents.get(url) {
            Some(d) => (d.tree.clone(), d.content.clone(), d.class_name.clone()),
            None => return,
        };

        let class_id = match self.classes.get(&class_name).copied() {
            Some(id) => id,
            None => return,
        };

        let Some(class_def_node) = tree
            .root_node()
            .named_child(tree.root_node().named_child_count() - 1)
        else {
            return;
        };

        let children = get_node_children(class_def_node);

        let mut inherited = Vec::new();
        if children.len() > 3 {
            for node in children[2..].iter() {
                if node.kind() == "class_extends" {
                    let inherited_nodes = get_node_children(*node);
                    for inh in inherited_nodes[1..].iter() {
                        let name = content.as_str()[inh.byte_range()].to_string();
                        if let Some(id) = self.classes.get(&name).copied() {
                            inherited.push(id);
                        }
                    }
                }
            }
        }

        if let Some(class) = self.global_semantic_model.classes.get_mut(class_id.0) {
            class.inherited_classes = inherited;
        }
    }

    fn get_pub_variable_symbol(&self, symbol_name: &str) -> Vec<(Url, Range)> {
        let mut locations = Vec::new();
        let Some(symbol_defs_by_class) = self.pub_var_defs.get(symbol_name) else {
            eprintln!("Couldn't find hashmap associated with given symbol name");
            return locations
        };
        for (class_name, symbols_defs) in symbol_defs_by_class {
            let Some(class_symbol_id) = self.class_defs.get(class_name) else {
                eprintln!("Couldn't find class symbol id given class name");
                continue;
            };
            for def in symbols_defs {
                let Some(variable_symbols_for_class) = self.global_semantic_model.variable_defs.get(class_symbol_id) else {
                    eprintln!("Couldn't find hashmap associated with given class symbol id");
                    continue;
                };
                if let Some(symbol) = variable_symbols_for_class.get(def.0) {
                    locations.push((symbol.url.clone(), symbol.location))
                }
                else {
                    eprintln!("Couldn't find symbol for given variable global symbol id");
                    continue;
                }
            }
        }
        locations
    }

    pub fn get_variable_symbol_location(&self, url: Url, point: Point, symbol_name: String, method_name: String) -> Vec<(Url, Range)>  {
        let mut locations = Vec::new();
        eprintln!("In symbol location method");
        let document = match self.documents.get(&url) {
            Some(d) => d,
            None => {
                eprintln!("Couldn't find document for given url");
                return locations
            },
        };

        let class_id = match document.class_id {
            Some(id) => id,
            None => return locations,
        };

        let Some(class) = self.global_semantic_model.classes.get(class_id.0) else {
            eprintln!("Couldn't find class for given class id");
            return locations;
        };

        if let Some(method_id) = class.public_methods.get(&method_name) {
            if let Some(methods) = self.global_semantic_model.methods.get(&class_id) {
                let Some(method) = methods.get(method_id.0) else {
                    eprintln!("Couldn't find method in global semantic model for given method id");
                    return locations;
                };
                if let Some(is_procedure_block) = method.is_procedure_block {
                    if is_procedure_block {
                        // find public variables
                        if method.public_variables_declared.contains(&symbol_name) {
                            locations = self.get_pub_variable_symbol(&symbol_name);
                            locations
                        }
                        else {
                           // variable is private
                            let Some(range) = document.scope_tree.get_variable_definition(point, symbol_name.as_str()) else {
                                eprintln!("Couldn't find symbol in scope tree for given variable name");
                                return locations
                            };
                            locations.push((url.clone(), range));
                            locations
                        }
                    }
                    else {
                        locations = self.get_pub_variable_symbol(&symbol_name);
                        locations
                    }
                }
                else if let Some(is_procedure_block) = class.is_procedure_block {
                    if is_procedure_block {
                        if method.public_variables_declared.contains(&symbol_name) {
                            locations = self.get_pub_variable_symbol(&symbol_name);
                            locations
                        }
                        else {
                            // variable is private
                            let Some(range) = document.scope_tree.get_variable_definition(point, symbol_name.as_str()) else {
                                eprintln!("Couldn't find symbol in scope tree for given variable name");
                                return locations
                            };
                            locations.push((url.clone(), range));
                            return locations;
                        }
                    }
                    else {
                        locations = self.get_pub_variable_symbol(&symbol_name);
                        return locations;
                    }
                }

                else {
                    // procedure block default
                    if method.public_variables_declared.contains(&symbol_name) {
                        locations = self.get_pub_variable_symbol(&symbol_name);
                        locations
                    }
                    else {
                        // variable is private
                        let Some(range) = document.scope_tree.get_variable_definition(point, symbol_name.as_str()) else {
                            eprintln!("Couldn't find symbol in scope tree for given variable name");
                            return locations
                        };
                        locations.push((url.clone(), range));
                        return locations;
                    }
                }
            }
            else {
                eprintln!("Couldn't find methods for class id in global semantic model");
                locations
            }
        }

        else if let Some(method_id) = class.private_methods.get(&method_name) {
            if let Some(local_semantic_model_id) = document.local_semantic_model_id {
                let Some(local_semantic_model) = self.global_semantic_model.private.get(local_semantic_model_id.0) else {
                    eprintln!("Couldn't find local semantic model for given local semantic id");
                    return locations;
                };
                let Some(method) = local_semantic_model.methods.get(method_id.0) else {
                    eprintln!("Couldn't find method in local semantic model");
                    return locations
                };

                if let Some(is_procedure_block) = method.is_procedure_block {
                    if is_procedure_block {
                        if method.public_variables_declared.contains(&symbol_name) {
                            locations = self.get_pub_variable_symbol(&symbol_name);
                            locations
                        }
                        else {
                            // variable is private
                            let Some(range) = document.scope_tree.get_variable_definition(point, symbol_name.as_str()) else {
                                eprintln!("Couldn't find symbol in scope tree for given variable name");
                                return locations
                            };
                            locations.push((url.clone(), range));
                            return locations;
                        }
                    }
                    else {
                        locations.extend(self.get_pub_variable_symbol(&symbol_name));
                        return locations;
                    }
                }

                else if let Some(is_procedure_block) = class.is_procedure_block {
                    if is_procedure_block {
                        if method.public_variables_declared.contains(&symbol_name) {
                            locations = self.get_pub_variable_symbol(&symbol_name);
                            locations
                        }
                        else {
                            // variable is private
                            let Some(range) = document.scope_tree.get_variable_definition(point, symbol_name.as_str()) else {
                                eprintln!("Couldn't find symbol in scope tree for given variable name");
                                return locations
                            };
                            locations.push((url.clone(), range));
                            return locations;
                        }
                    }
                    else {
                        locations.extend(self.get_pub_variable_symbol(&symbol_name));
                        return locations;
                    }
                }

                else {
                    // procedure block default
                    if method.public_variables_declared.contains(&symbol_name) {
                        locations = self.get_pub_variable_symbol(&symbol_name);
                        locations
                    }
                    else {
                        // variable is private
                        let Some(range) = document.scope_tree.get_variable_definition(point, symbol_name.as_str()) else {
                            eprintln!("Couldn't find symbol in scope tree for given variable name");
                            return locations
                        };
                        locations.push((url.clone(), range));
                        return locations;
                    }
                }

            }
            else {
                eprintln!("Couldn't find local semantic model id from the document");
                return locations;
            }
        }

        else {
            eprintln!("Method name was not found for this class.");
            return locations;
        }
    }

    pub fn get_method_overrides(&self, url: Url, method_name: String) -> Vec<(Url, Range)> {
        let mut locations = Vec::new();

        eprintln!("In Method Overrides");
        // ---- document ----
        let document = match self.documents.get(&url) {
            Some(d) => d,
            None => return locations,
        };

        eprintln!("Got document {:?}", document);
        let class_id = match document.class_id {
            Some(id) => id,
            None => return locations,
        };

        eprintln!("Got class {:?}", class_id);
        // ---- base method ----
        let method_id = match self
            .global_semantic_model
            .classes
            .get(class_id.0)
            .and_then(|c| c.public_methods.get(&method_name))
        {
            Some(id) => *id,
            None => return locations, // not a public method → no overrides
        };

        let method_ref = PublicMethodRef {
            class: class_id,
            id: method_id,
        };

        // ---- overridden-by list ----
        let overrides = match self.override_index.overridden_by.get(&method_ref) {
            Some(v) => v,
            None => return locations,
        };

        eprintln!("Overrides for {:?}", overrides);

        for method in overrides {
            let class = match self.global_semantic_model.classes.get(method.class.0) {
                Some(c) => c,
                None => continue,
            };

            let cls_name = &class.name;

            if let Some(_) = method.pub_id {
                eprintln!("Public method for {:?}", cls_name);
                // get the overriding class's symbol id
                let child_class_symbol_id = match self.class_defs.get(cls_name).copied() {
                    Some(id) => id,
                    None => continue,
                };
                let method_sym_id = match self
                    .pub_method_defs
                    .get(cls_name)
                    .and_then(|m| m.get(&method_name))
                {
                    Some(id) => *id,
                    None => continue,
                };

                let sym = match self
                    .global_semantic_model
                    .method_defs
                    .get(&child_class_symbol_id)
                    .and_then(|v| v.get(method_sym_id.0))
                {
                    Some(s) => s,
                    None => continue,
                };

                locations.push((sym.url.clone(), sym.location));
            } else {
                // ---------- private override ----------
                let cls_symbol_id = match self.class_defs.get(cls_name) {
                    Some(id) => *id,
                    None => continue,
                };

                let cls_url = &self.global_semantic_model.class_defs[cls_symbol_id.0].url;

                let doc = match self.documents.get(cls_url) {
                    Some(d) => d,
                    None => continue,
                };

                if let Some(sym) = doc
                    .scope_tree
                    .get_private_method_symbol(method_name.clone())
                {
                    locations.push((cls_url.clone(), sym.location));
                }
            }
        }
        locations
    }
}

impl ProjectState {
    pub fn new() -> Self {
        Self {
            project_root_path: OnceLock::new(),
            parsers: WorkspaceParsers::new(),
            data: RwLock::new(ProjectData {
                config: Config::default(),
                documents: HashMap::new(),
                global_semantic_model: GlobalSemanticModel::new(),
                classes: HashMap::new(),
                class_defs: HashMap::new(),
                pub_method_defs: HashMap::new(),
                pub_var_defs: HashMap::new(),
                override_index: OverrideIndex::new(),
            }),
        }
    }

    pub fn handle_document_opened(
        &self,
        url: Url,
        text: String,
        file_type: FileType,
        version: i32,
    ) {
        // Parse OUTSIDE lock
        let tree = if file_type == FileType::Cls {
            match self.parsers.cls.lock().parse(&text, None) {
                Some(t) => t,
                None => {
                    eprintln!("parse failed for CLS: {}", url);
                    return;
                }
            }
        } else {
            match self.parsers.routine.lock().parse(&text, None) {
                Some(t) => t,
                None => {
                    eprintln!("parse failed for routine: {}", url);
                    return;
                }
            }
        };

        let class_name = if file_type == FileType::Cls {
            get_class_name_from_root(&text, tree.root_node())
        } else {
            "TODO".to_string()
        };

        // Commit INSIDE one lock
        let mut data = self.data.write();

        let existing_snapshot = data
            .documents
            .get(&url)
            .map(|d| (d.content.clone(), d.file_type.clone()));

        match existing_snapshot {
            None => {
                data.add_document(
                    url.clone(),
                    text,
                    tree,
                    file_type,
                    class_name,
                    Some(version),
                );
                // IMPORTANT: build override index/calls/vars for new doc too
                data.build_inheritance_and_variables(Some(url));
            }
            Some((old_text, old_type)) => {
                if old_text != text || old_type != file_type {
                    data.update_document(url, tree, file_type, version, &text);
                } else {
                    if let Some(doc) = data.documents.get_mut(&url) {
                        doc.version = Some(version);
                    }
                }
            }
        }
    }

    pub fn get_document_info(&self, url: &Url) -> Option<(FileType, String, i32, Tree)> {
        self.data.read().get_document_info(url)
    }

    pub fn update_document(
        &self,
        url: Url,
        tree: Tree,
        file_type: FileType,
        version: i32,
        content: &str,
    ) {
        self.data
            .write()
            .update_document(url, tree, file_type, version, content);
    }

    pub fn root_path(&self) -> Option<&std::path::Path> {
        self.project_root_path.get().and_then(|o| o.as_deref())
    }
}
