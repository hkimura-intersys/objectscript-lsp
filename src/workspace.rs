use crate::common::{
    build_method_calls, build_method_calls_from_unresolved, find_class_definition,
    generic_exit_statements, generic_skipping_statements, get_class_name_from_root,
    get_node_children, initial_build_scope_tree, print_statements_exit_method_overrides_fn,
    start_of_function, successful_exit,
};
use crate::config::Config;
use crate::document::Document;
use crate::global_semantic::GlobalSemanticModel;
use crate::override_index::OverrideIndex;
use crate::parse_structures::{
    Class, ClassId, FileType, Language, LocalSemanticModelId, MethodCallSite,
    PrivateMethodId, PublicMethodId, PublicMethodRef,
};
use crate::scope_structures::{
    ClassGlobalSymbolId, MethodGlobalSymbol, MethodGlobalSymbolId, VariableGlobalSymbolId,
};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::OnceLock;
use tower_lsp::lsp_types::Url;
use tree_sitter::{Node, Parser, Point, Range, Tree};
use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};
use crate::local_semantic::LocalSemanticModel;

pub struct WorkspaceParsers {
    pub(crate) routine: Mutex<Parser>,
    pub(crate) cls: Mutex<Parser>,
}

impl Debug for WorkspaceParsers {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl WorkspaceParsers {
    /// Construct a `WorkspaceParsers` with both ObjectScript grammars initialized.
    ///
    /// - `cls` uses the full class grammar (`LANGUAGE_OBJECTSCRIPT`)
    /// - `routine` uses the core/routine grammar (`LANGUAGE_OBJECTSCRIPT_CORE`)
    ///
    /// Panics if either grammar fails to load (intended to fail-fast during startup).
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

/// Stores all workspace-wide state needed to serve LSP features.
///
/// `ProjectData` is the in-memory “database” for a single workspace: it owns the
/// current configuration, parsed documents, semantic models, and symbol indexes
/// used for lookups like go-to-definition, references, and override resolution.
///
#[derive(Debug)]
pub struct ProjectData {
    /// Stores the User Settings for this Workspace.
    pub(crate) config: Config,
    /// Maps Url -> Document for each `.cls`, `.mac`, and `.inc` document in this Workspace.
    pub(crate) documents: HashMap<Url, Document>,
    /// Stores all semantic information for this Workspace.
    pub(crate) global_semantic_model: GlobalSemanticModel,
    /// Maps class name -> ClassId(index) for each class in this workspace.
    pub(crate) classes: HashMap<String, ClassId>,
    /// Maps class name -> ClassGlobalSymbolId(Index) for each class in this workspace.
    pub(crate) class_defs: HashMap<String, ClassGlobalSymbolId>,
    /// Maps Class Name -> another hashmap which maps Method Name -> MethodGlobalSymbolId for all public methods
    pub(crate) pub_method_defs: HashMap<String, HashMap<String, MethodGlobalSymbolId>>,
    /// Maps Var Name -> another hashmap which maps Class name -> a list of VariableGlobalSymbolId for that variable.
    pub(crate) pub_var_defs: HashMap<String, HashMap<String, Vec<VariableGlobalSymbolId>>>,
    /// Holds the OverrideIndex for the workspace.
    pub(crate) override_index: OverrideIndex,
}

/// Concurrency wrapper for a workspace’s state and parsers.
///
/// `ProjectState` holds the project root path and a lock-protected `ProjectData`,
/// along with Tree-sitter parsers shared across requests. This is the primary
/// entry point for workspace-level operations (open/update/index).
#[derive(Debug)]
pub struct ProjectState {
    /// Workspace root path (set once during initialize()).
    pub(crate) project_root_path: OnceLock<Option<PathBuf>>,
    /// Lock-protected workspace data (documents, semantics, symbols, indexes).
    pub(crate) data: RwLock<ProjectData>,
    /// Reusable parsers for `.cls` and routine files.
    pub(crate) parsers: WorkspaceParsers,
}

impl ProjectData {
    /// Return basic immutable snapshot information for a document.
    ///
    /// Produces `(file_type, content, version, tree)` for the document at `url`. The text and tree
    /// are cloned so callers can use them without holding a borrow on `ProjectData`.
    ///
    /// Returns `None` if the document is not currently tracked.
    pub fn get_document_info(&self, url: &Url) -> Option<(FileType, String, i32, Tree)> {
        // start_of_function("ProjectData", "get_document_info");
        let Some(document) = self.get_document(url) else {
            generic_exit_statements("ProjectData", "get_document_info");
            return None;
        };
        let curr_version = document.version.unwrap_or(0);
        let current_text = document.content.clone();
        let curr_tree = document.tree.clone();
        // successful_exit("ProjectData", "get_document_info");
        Some((
            document.file_type.clone(),
            current_text,
            curr_version,
            curr_tree,
        ))
    }

    /// Add a document only if it is not already present.
    /// Returns true if the document was present, false otherwise.
    pub fn add_document_if_absent(
        &mut self,
        url: Url,
        code: String,
        tree: Tree,
        filetype: FileType,
        class_name: String,
        version: Option<i32>,
    ) -> bool {
        start_of_function("ProjectData", "add_document_if_absent");
        if self.documents.contains_key(&url) {
            eprintln!("Document already exists for file at :{:?}", url.path());
            successful_exit("ProjectData", "add_document_if_absent");
            return true;
        }
        self.add_document(url, code, tree, filetype, class_name, version);
        successful_exit("ProjectData", "add_document_if_absent");
        false
    }

    /// Parse and register a new document, initializing semantic + symbol state for `.cls` files.
    ///
    /// For class files (`FileType::Cls`), this:
    /// - Extracts the class definition/range
    /// - Builds an initial `Class` and method list from the tree-sitter tree
    /// - Creates a `ClassGlobalSymbol`, `ScopeTree`, and `Document`
    /// - Adds public methods into the global semantic model and method symbol tables
    /// - Adds private methods into the local semantic model and scope tree symbols
    /// - Registers class ids and local semantic model ids for later rebuilds
    ///
    /// Non-CLS file types are currently ignored by this function.
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
            start_of_function("ProjectData", "add_document");
            let Some(node) = find_class_definition(tree.root_node()) else {
                generic_exit_statements("ProjectData", "add_document");
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
                    let Some(method_symbol_id) = self.global_semantic_model.new_method_symbol(
                        method_name.clone(),
                        range,
                        url.clone(),
                        class_symbol_id,
                    ) else {
                        generic_skipping_statements(
                            "add_document",
                            method_name.as_str(),
                            "Method Symbol Named",
                        );
                        continue;
                    };
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

            successful_exit("ProjectData", "add_document");
        }
    }

    /// Update a tracked document after text edits or reparse.
    ///
    /// This function:
    /// - Re-parses/derives the current class name from the new `tree` + `content`
    /// - Rebuilds the document's scope tree
    /// - Clears old symbol/semantic state for the document (class/method/variable symbols, local model)
    /// - Rebuilds class + method headers into semantic models (`rebuild_semantics`)
    /// - Updates the stored `Document` fields (content/tree/version/type/name)
    /// - Recomputes imports, inheritance, overrides, calls, and variables for the project
    pub fn update_document(
        &mut self,
        url: Url,
        tree: Tree,
        file_type: FileType,
        version: i32,
        content: &str,
    ) {
        start_of_function("ProjectData", "update_document");
        // println!("---------------------------------");
        // println!("Before Update:");
        // println!("---------------------------------");
        // println!("Public Variables: {:#?}", self.pub_var_defs);
        // println!("Variable Defs in GSM: {:#?}", self.global_semantic_model.variable_defs);
        // println!("Class Name -> ID {:#?}", self.class_defs);
        let Some(class_name) = get_class_name_from_root(content, tree.root_node()) else {
            eprintln!("Warning: Failed to get class name from root node for file with the following content: {:?}", content);
            generic_exit_statements("ProjectData", "update_document");
            return;
        };

        let (class_symbol_id, class_id, local_semantic_id, old_class_name) = {
            let Some(doc) = self.get_document(&url) else {
                generic_exit_statements("ProjectData", "update_document");
                return;
            };
            let Some(class_id) = doc.class_id else {
                eprintln!(
                    "Error: Cannot update document, no class id found in document for url: {}",
                    url.path()
                );
                generic_exit_statements("ProjectData", "update_document");
                return;
            };

            let Some(local_semantic_model_id) = doc.local_semantic_model_id else {
                eprintln!(
                    "Warning: No local_semantic_model_id found in document for url: {}",
                    url.path()
                );
                generic_exit_statements("ProjectData", "update_document");
                return;
            };
            (
                doc.scope_tree.class_def,
                class_id,
                local_semantic_model_id,
                doc.class_name.clone(),
            )
        };

        // Update scope tree
        {
            let Some(doc) = self.get_document_mut(&url) else {
                generic_exit_statements("ProjectData", "update_document");
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

        self.class_defs.remove(&old_class_name);
        self.pub_method_defs.remove(&old_class_name);
        self.classes.remove(&old_class_name);

        self.classes.insert(class_name.clone(), class_id);
        self.class_defs.insert(class_name.clone(), class_symbol_id);

        {
            for (_, class_map) in &mut self.pub_var_defs {
                if class_map.contains_key(&old_class_name) {
                    class_map.remove(&old_class_name);
                }
            }
        }

        let Some(node) = tree
            .root_node()
            .named_child(tree.root_node().named_child_count() - 1)
        else {
            eprintln!(
                "Failed to get class definition node from tree for content {:?}",
                content
            );
            generic_exit_statements("ProjectData", "update_document");
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
            file_type.clone(),
        );

        // Update document fields in another short mutable borrow
        {
            let Some(document) = self.get_document_mut(&url) else {
                generic_exit_statements("ProjectData", "update_document");
                return;
            };
            document.version = Some(version);
            document.file_type = file_type;
            document.tree = tree;
            document.content = content.to_string();
            document.class_name = class_name;
        }

        // Recompute inheritance/override/calls/vars
        self.build_inheritance_and_variables(Some(url), Vec::new());

        successful_exit("ProjectData", "update_document");
    }

    /// Rebuild class + method header semantics for a document after a reparse.
    ///
    /// This reconstructs the `Class` for `class_id` from the given class definition `node`, then:
    /// - Updates the class symbol (name/range/url)
    /// - Re-registers public methods and method symbols into the global semantic model
    /// - Re-registers private methods into the local semantic model and scope tree
    /// - Replaces the class slot in the global semantic model at `class_id`
    ///
    /// Note: This function does not rebuild statement-level variables/calls; those are handled by
    /// `build_inheritance_and_variables`.
    pub fn rebuild_semantics(
        &mut self,
        url: Url,
        node: Node,
        content: &str,
        class_id: ClassId,
        class_symbol_id: ClassGlobalSymbolId,
        local_semantic_model_id: LocalSemanticModelId,
        class_name: String,
        file_type: FileType,
    ) {
        start_of_function("ProjectData", "rebuild_semantics");
        if file_type != FileType::Cls {
            eprintln!("File Type not yet implemented");
            generic_exit_statements("ProjectData", "rebuild_semantics");
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
                let Some(method_symbol_id) = self.global_semantic_model.new_method_symbol(
                    method_name.clone(),
                    range,
                    url.clone(),
                    class_symbol_id,
                ) else {
                    generic_skipping_statements(
                        "rebuild_semantics",
                        method_name.as_str(),
                        "Method Symbol for method named",
                    );
                    continue;
                };
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
                let Some(document) = self.get_document_mut(&url) else {
                    generic_exit_statements("ProjectData", "rebuild_semantics");
                    return;
                };
                document
                    .scope_tree
                    .new_method_symbol(method_name.clone(), range);
            }
        }

        if let Some(slot) = self.global_semantic_model.classes.get_mut(class_id.0) {
            *slot = class;
        } else {
            eprintln!(
                "Warning: tried to assign class to classes vec in global semantic model, but index {:?} is out of bounds (len={})",
                class_id.0,
                self.global_semantic_model.classes.len()
            );
            generic_exit_statements("ProjectData", "rebuild_semantics");
            return;
        }

        for method in gsm_methods {
            self.global_semantic_model.new_method(method, class_id);
        }

        let Some(local_semantic_model) = self
            .global_semantic_model
            .get_local_semantic_mut(local_semantic_model_id)
        else {
            generic_exit_statements("ProjectData", "rebuild_semantics");
            return;
        };
        for method in lsm_methods {
            local_semantic_model.new_method(method);
        }
        local_semantic_model.active = true;
        let Some(doc) = self.get_document_mut(&url) else {
            generic_exit_statements("ProjectData", "rebuild_semantics");
            return;
        };
        // this creates the symbol and adds the symbol id to the scope tree
        doc.local_semantic_model_id = Some(local_semantic_model_id);
        doc.class_id = Some(class_id);
        successful_exit("ProjectData", "rebuild_semantics");
    }

    /// Compute imports, inheritance, override resolution, call sites, and variable symbols.
    ///
    /// If `only` is provided, only that document is scanned for import/extends changes; the
    /// inheritance/override index is still rebuilt globally, and method calls/variables are
    /// recomputed for all classes in the semantic model.
    pub fn build_inheritance_and_variables(&mut self, only: Option<Url>, exclude: Vec<Url>) {
        start_of_function("ProjectData", "build_inheritance_and_variables");
        let mut indices_to_exclude = Vec::new();
        // Which documents should update imports/extends?
        if let Some(url) = only {
            self.recompute_imports_for_url(&url);
            self.recompute_extends_for_url(&url);
            if exclude.contains(&url) {
                eprintln!("Error: Url specified as only one to change is also included in the exclude field.");
                generic_exit_statements("ProjectData", "build_inheritance_and_variables");
                return;
            }
            let Some(document) = self.documents.get(&url) else {
                eprintln!("Error: Failed to get document for url {:?}", url.path());
                generic_exit_statements("ProjectData", "build_inheritance_and_variables");
                return;
            };
            let Some(index) = document.class_id else {
                eprintln!(
                    "Error: Failed to get class id from document. {:?}",
                    url.path()
                );
                generic_exit_statements("ProjectData", "build_inheritance_and_variables");
                return;
            };
            indices_to_exclude = (0..self.global_semantic_model.classes.len())
                .filter(|&i| i != index.0)
                .collect();
        } else {
            let urls: Vec<Url> = self
                .documents
                .keys()
                .cloned()
                .into_iter()
                .filter(|url| !exclude.contains(url))
                .collect();
            for url in &urls {
                self.recompute_imports_for_url(url);
                self.recompute_extends_for_url(url);
            }
            for url in &exclude {
                let Some(document) = self.documents.get(url) else {
                    eprintln!("Error: Tried to get document to exclude this class from being rebuilt. Failed to get document for url {:?}", url.path());
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        url.path(),
                        "document",
                    );
                    continue;
                };
                let Some(index) = document.class_id else {
                    eprintln!("Error: Tried to exclude class from being rebuilt. Failed to get class id from document. {:?}", url.path());
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        document.class_name.as_str(),
                        "document",
                    );
                    continue;
                };

                indices_to_exclude.push(index.0);
            }
        }

        // Recompute inheritance + override index
        self.global_semantic_model.class_keyword_inheritance();
        let idx = self.global_semantic_model.build_override_index();
        self.override_index = idx.clone();

        // need to calculate which classes to actually rebuild semantics for

        // TODO: update this
        for c in &mut self.global_semantic_model.classes {
            c.method_calls.clear();
        }

        let class_len = self.global_semantic_model.classes.len();
        let classes_map = self.classes.clone();

        for i in 0..class_len {
            let (
                class_name,
                public_method_ids,
                private_method_ids,
                is_procedure_block,
                default_language,
            ) = {
                let Some(class) = self.global_semantic_model.get_class(i) else {
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        "Couldn't find",
                        "class",
                    );
                    continue;
                };
                eprintln!(
                    "Info: Building method keyword inheritance and variables for class: {:?}",
                    class.name
                );
                let is_procedure_block = if class.is_procedure_block.is_none() {
                    false
                } else {
                    class.is_procedure_block.unwrap()
                };

                let default_language = if class.default_language.is_none() {
                    Language::Objectscript
                } else {
                    class.default_language.clone().unwrap()
                };
                (
                    class.name.clone(),
                    class.public_methods.values().cloned().collect::<Vec<_>>(),
                    class.private_methods.values().cloned().collect::<Vec<_>>(),
                    is_procedure_block,
                    default_language,
                )
            };

            if indices_to_exclude.contains(&i) {
                generic_skipping_statements(
                    "build_inheritance_and_variables",
                    class_name.as_str(),
                    "class",
                );
                continue;
            }

            let class_symbol_id = match self.class_defs.get(&class_name).copied() {
                Some(id) => id,
                None => {
                    eprintln!(
                        "Warning: Couldn't find class symbol id for class named {:?}.",
                        class_name
                    );
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        class_name.as_str(),
                        "class",
                    );
                    continue;
                }
            };
            let class_id = match self.classes.get(&class_name).copied() {
                Some(id) => id,
                None => {
                    eprintln!(
                        "Warning: Couldn't find class id for class named {:?}",
                        class_name
                    );
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        class_name.as_str(),
                        "class",
                    );
                    continue;
                }
            };

            let url = {
                let Some(class_global_symbol) = self
                    .global_semantic_model
                    .get_class_symbol(class_symbol_id.0, class_name.as_str())
                else {
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        class_name.as_str(),
                        "class",
                    );
                    continue;
                };

                class_global_symbol.url.clone()
            };

            let (content, tree, scope_tree_snapshot, local_semantic_id) = {
                let Some(document) = self.get_document(&url) else {
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        class_name.as_str(),
                        "class",
                    );
                    continue;
                };
                let content = document.content.clone();
                let tree = document.tree.clone();
                let scope_tree_snapshot = document.scope_tree.clone(); // for private method symbol lookup
                let local_semantic_id = match document.local_semantic_model_id {
                    Some(id) => id,
                    None => {
                        eprintln!(
                            "Warning: No local semantic model found in document for class named: {:?}",
                            document.class_name
                        );
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            class_name.as_str(),
                            "class",
                        );
                        continue;
                    }
                };
                (content, tree, scope_tree_snapshot, local_semantic_id)
            };
            let content = content.as_str();
            let tree_root_node = tree.root_node();

            // ---------- public methods ----------
            for pub_method_id in public_method_ids {
                // inherit class keywords if not explicitly assigned
                {
                    let Some(method) = self.global_semantic_model.get_mut_method(
                        class_id,
                        class_name.as_str(),
                        pub_method_id.0,
                    ) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            "Couldn't Find Method",
                            "Method",
                        );
                        continue;
                    };

                    method.update_keywords(is_procedure_block, default_language.clone());
                }

                let (method_name, loc) = {
                    let Some(method) = self.global_semantic_model.get_method(
                        class_id,
                        class_name.as_str(),
                        pub_method_id.0,
                    ) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            "Couldn't Find Method",
                            "Method",
                        );
                        continue;
                    };
                    let method_name = method.name.clone();
                    let Some(sym) = self.get_public_method_symbol(
                        class_name.as_str(),
                        method_name.as_str(),
                        class_symbol_id,
                    ) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            method_name.as_str(),
                            "Method",
                        );
                        continue;
                    };

                    (method_name, sym.location)
                };

                let method_name = method_name.as_str();

                eprintln!(
                    "Info: Building inheritance for variables in public method {:?}",
                    method_name
                );

                let Some(method_definition_node) =
                    tree_root_node.named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                else {
                    eprintln!(
                        "Warning: Failed to get method definition node from tree: {:?}",
                        tree_root_node
                    );
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        method_name,
                        "Method",
                    );
                    continue;
                };

                // method Calls
                let calls = build_method_calls(&class_name, method_definition_node, content);

                let new_sites: Vec<MethodCallSite> = build_method_calls_from_unresolved(
                    classes_map.clone(),
                    idx.clone(),
                    calls,
                    String::from(method_name),
                );

                self.global_semantic_model.classes[i]
                    .method_calls
                    .extend(new_sites);

                // Variables: compute first (immutable), then apply (mutable) to avoid long borrows
                let var_results = {
                    let Some(method) = self.global_semantic_model.get_method(
                        class_id,
                        class_name.as_str(),
                        pub_method_id.0,
                    ) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            method_name,
                            "Method",
                        );
                        continue;
                    };
                    method.build_method_variables_and_ref(method_definition_node, content)
                };
                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in
                    var_results
                {
                    let var_name = variable.name.clone();
                    if refs_to_other_vars.contains(&var_name) {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            var_name.as_str(),
                            "Variable",
                        );
                        continue;
                    }

                    if variable.is_public {
                        let var_id = self.global_semantic_model.new_variable(variable, &class_id);

                        {
                            let Some(method) = self.global_semantic_model.get_mut_method(
                                class_id,
                                class_name.as_str(),
                                pub_method_id.0,
                            ) else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            method.public_variables.insert(var_name.clone(), var_id);
                        }

                        let Some(symbol_id) = self.global_semantic_model.new_variable_symbol(
                            var_name.clone(),
                            variable_range,
                            url.clone(),
                            refs_to_other_vars.clone(),
                            refs_to_properties.clone(),
                            class_symbol_id,
                        ) else {
                            generic_skipping_statements(
                                "build_inheritance_and_variables",
                                var_name.as_str(),
                                "Variable",
                            );
                            continue;
                        };

                        {
                            let Some(document) = self.get_document_mut(&url) else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            document.scope_tree.new_public_var_symbol(
                                var_name.clone(),
                                variable_range,
                                symbol_id,
                            );
                        }

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
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            lsm.new_variable(variable)
                        };

                        {
                            let Some(method) = self.global_semantic_model.get_mut_method(
                                class_id,
                                class_name.as_str(),
                                pub_method_id.0,
                            ) else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            method.private_variables.insert(var_name.clone(), var_id);
                        }

                        {
                            let Some(document) = self.get_document_mut(&url) else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            document.scope_tree.new_variable_symbol(
                                var_name,
                                variable_range,
                                refs_to_other_vars,
                                refs_to_properties,
                            );
                        }
                    }
                }
            }

            // ---------- private methods ----------
            for private_method_id in private_method_ids {
                {
                    // inherit class keywords if not specified in method
                    let Some(lsm) = self
                        .global_semantic_model
                        .get_local_semantic_mut(local_semantic_id)
                    else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            "Method not yet obtained.",
                            "Method",
                        );
                        continue;
                    };
                    let Some(method) = lsm.get_method_mut(private_method_id.0) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            "Method not found.",
                            "Method",
                        );
                        continue;
                    };

                    // inherit class keywords if not explicitly assigned
                    method.update_keywords(is_procedure_block, default_language.clone());
                }
                let (method_name, loc) = {
                    let Some(lsm) = self
                        .global_semantic_model
                        .get_local_semantic(local_semantic_id)
                    else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            "Method not yet obtained.",
                            "Method",
                        );
                        continue;
                    };
                    let Some(m) = lsm.get_method(private_method_id.0) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            "Method not found.",
                            "Method",
                        );
                        continue;
                    };

                    let Some(sym) = scope_tree_snapshot.get_private_method_symbol(m.name.as_str())
                    else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            m.name.as_str(),
                            "Method",
                        );
                        continue;
                    };

                    (m.name.clone(), sym.location)
                };
                eprintln!(
                    "Info: Building inheritance for variables in private method {:?}",
                    method_name
                );

                let Some(method_definition_node) =
                    tree_root_node.named_descendant_for_byte_range(loc.start_byte, loc.end_byte)
                else {
                    eprintln!(
                        "Failed to get method definition node for method named {:?}",
                        method_name
                    );
                    generic_skipping_statements(
                        "build_inheritance_and_variables",
                        method_name.as_str(),
                        "Method",
                    );
                    continue;
                };

                let calls = build_method_calls(&class_name, method_definition_node, content);

                let new_sites: Vec<MethodCallSite> = build_method_calls_from_unresolved(
                    classes_map.clone(),
                    idx.clone(),
                    calls,
                    method_name.clone(),
                );

                self.global_semantic_model.classes[i]
                    .method_calls
                    .extend(new_sites);

                let var_results = {
                    let Some(local_semantic_model) = self
                        .global_semantic_model
                        .get_local_semantic(local_semantic_id)
                    else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            method_name.as_str(),
                            "Method",
                        );
                        continue;
                    };
                    let Some(method) = local_semantic_model.get_method(private_method_id.0) else {
                        generic_skipping_statements(
                            "build_inheritance_and_variables",
                            method_name.as_str(),
                            "Method",
                        );
                        continue;
                    };
                    method.build_method_variables_and_ref(method_definition_node, content)
                };

                for (variable, variable_range, refs_to_other_vars, refs_to_properties) in
                    var_results
                {
                    let var_name = variable.name.clone();
                    if refs_to_other_vars.contains(&var_name) {
                        eprintln!(
                            "Skipping var named {:?}, definition contains ref to itself",
                            var_name
                        );
                        continue;
                    }

                    if variable.is_public {
                        let var_id = self.global_semantic_model.new_variable(variable, &class_id);

                        {
                            let Some(local_semantic_model) = self
                                .global_semantic_model
                                .get_local_semantic_mut(local_semantic_id)
                            else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            let Some(method) =
                                local_semantic_model.get_method_mut(private_method_id.0)
                            else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            method.public_variables.insert(var_name.clone(), var_id);
                        }

                        let Some(symbol_id) = self.global_semantic_model.new_variable_symbol(
                            var_name.clone(),
                            variable_range,
                            url.clone(),
                            refs_to_other_vars.clone(),
                            refs_to_properties.clone(),
                            class_symbol_id,
                        ) else {
                            generic_skipping_statements(
                                "build_inheritance_and_variables",
                                var_name.as_str(),
                                "Variable",
                            );
                            continue;
                        };

                        {
                            let Some(document) = self.get_document_mut(&url) else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            document.scope_tree.new_public_var_symbol(
                                var_name.clone(),
                                variable_range,
                                symbol_id,
                            );
                        }

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
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            lsm.new_variable(variable)
                        };

                        {
                            let Some(local_semantic_model) = self
                                .global_semantic_model
                                .get_local_semantic_mut(local_semantic_id)
                            else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            let Some(method) =
                                local_semantic_model.get_method_mut(private_method_id.0)
                            else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            method.private_variables.insert(var_name.clone(), var_id);
                        }
                        {
                            let Some(document) = self.get_document_mut(&url) else {
                                generic_skipping_statements(
                                    "build_inheritance_and_variables",
                                    var_name.as_str(),
                                    "Variable",
                                );
                                continue;
                            };
                            document.scope_tree.new_variable_symbol(
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
        successful_exit("ProjectData", "build_inheritance_and_variables");
    }

    /// Recomputes the import list for the class defined in `url`.
    ///
    /// This scans the non-class-definition portion of the file (everything before the
    /// trailing `class_definition` node) for `import_code` statements, resolves imported
    /// class names to `ClassId`s using `self.classes`, and updates the corresponding
    /// `Class.imports` entry in the global semantic model.
    ///
    /// If the document or owning class cannot be found, the function logs a warning and
    /// returns early without modifying state.
    fn recompute_imports_for_url(&mut self, url: &Url) {
        start_of_function("ProjectData", "recompute_imports_for_url");
        let (tree, content, class_name) = match self.get_document(url) {
            Some(d) => (d.tree.clone(), d.content.clone(), d.class_name.clone()),
            None => {
                generic_exit_statements("ProjectData", "recompute_imports_for_url");
                return;
            }
        };

        let class_id = match self.classes.get(&class_name).copied() {
            Some(id) => id,
            None => {
                eprintln!("Failed to get class id for class named {:?}", class_name);
                generic_exit_statements("ProjectData", "recompute_imports_for_url");
                return;
            }
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
        successful_exit("ProjectData", "recompute_imports_for_url");
    }

    /// Recompute direct `extends` (inheritance) dependencies for the class defined in `url`.
    ///
    /// Parses the class definition's `class_extends` entries and updates `class.inherited_classes`
    /// with direct parent `ClassId`s (when resolvable). This should be run before building the
    /// override index, which assumes direct parents only.
    fn recompute_extends_for_url(&mut self, url: &Url) {
        start_of_function("ProjectData", "recompute_extends_for_url");
        let (tree, content, class_name) = match self.get_document(url) {
            Some(d) => (d.tree.clone(), d.content.clone(), d.class_name.clone()),
            None => {
                generic_exit_statements("ProjectData", "recompute_extends_for_url");
                return;
            }
        };

        let class_id = match self.classes.get(&class_name).copied() {
            Some(id) => id,
            None => {
                eprintln!("Failed to get class id for class named {:?}", class_name);
                generic_exit_statements("ProjectData", "recompute_extends_for_url");
                return;
            }
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
        successful_exit("ProjectData", "recompute_extends_for_url");
    }

    /// Fetch a tracked document by URL.
    ///
    /// Returns `None` and logs an error if the URL is not present in `self.documents`.
    fn get_document(&self, url: &Url) -> Option<&Document> {
        let Some(document) = self.documents.get(url) else {
            eprintln!("Error: Couldn't find document for url: {}", url.path());
            return None;
        };
        Some(document)
    }

    /// Fetch a tracked document by URL as a mutable reference.
    ///
    /// Returns `None` and logs an error if the URL is not present in `self.documents`.
    fn get_document_mut(&mut self, url: &Url) -> Option<&mut Document> {
        let Some(document) = self.documents.get_mut(url) else {
            eprintln!("Error: Couldn't find document for url: {}", url.path());
            return None;
        };
        Some(document)
    }

    /// Lookup the global symbol (name/range/url) for a public method in a class.
    ///
    /// This first resolves the method's symbol id from `pub_method_defs[class_name][method_name]`,
    /// then retrieves the `MethodGlobalSymbol` from the global semantic model.
    fn get_public_method_symbol(
        &self,
        class_name: &str,
        method_name: &str,
        class_symbol_id: ClassGlobalSymbolId,
    ) -> Option<&MethodGlobalSymbol> {
        let Some(&sym_id) = self
            .pub_method_defs
            .get(class_name)
            .and_then(|m| m.get(method_name))
        else {
            eprintln!(
                "Warning: Failed to get public method symbol id from method locations hashmap: {:?}, for class named {:?}",
                self.pub_method_defs.get(class_name).clone(),
                class_name
            );
            return None;
        };

        let Some(sym) =
            self.global_semantic_model
                .get_method_symbol(class_symbol_id, class_name, sym_id.0)
        else {
            return None;
        };
        Some(sym)
    }

    /// Try to resolve a public variable definition in the current scope only.
    ///
    /// This checks the document's `ScopeTree` at `point` to see if `symbol_name` is mapped to a
    /// `VariableGlobalSymbolId` in the current scope. If found, it looks up the global symbol and
    /// returns `(url, range)`.
    ///
    /// Returns `None` if the symbol is not public in this scope or cannot be resolved.
    fn get_pub_var_symbol_from_current_scope(
        &self,
        symbol_name: &str,
        url: &Url,
        point: Point,
    ) -> Option<(Url, Range)> {
        start_of_function("ProjectData", "get_pub_var_symbol_from_current_scope");
        let (class_symbol_id, var_symbol_id, class_name) = {
            let Some(document) = self.get_document(url) else {
                generic_exit_statements("ProjectData", "get_pub_var_symbol_from_current_scope");
                return None;
            };
            let Some((class_symbol_id, var_symbol_id)) = document
                .scope_tree
                .pub_variable_in_scope(point, symbol_name)
            else {
                generic_exit_statements("ProjectData", "get_pub_var_symbol_from_current_scope");
                return None;
            };

            (class_symbol_id, var_symbol_id, document.class_name.as_str())
        };

        let Some(var_symbol) = self.global_semantic_model.get_variable_symbol(
            &class_symbol_id,
            var_symbol_id.0,
            class_name,
        ) else {
            generic_exit_statements("ProjectData", "get_pub_var_symbol_from_current_scope");
            return None;
        };
        if url.clone() != var_symbol.url.clone() {
            eprintln!("ERROR: Expected Url and Var Symbol URL to be the same. URL: {:?}, VAR SYMBOL URL: {:?}", url.path(), var_symbol.url.path());
            generic_exit_statements("ProjectData", "get_pub_var_symbol_from_current_scope");
            return None;
        }
        successful_exit("ProjectData", "get_pub_var_symbol_from_current_scope");
        Some((var_symbol.url.clone(), var_symbol.location))
    }

    /// Resolve a public variable symbol to one or more definition locations.
    ///
    /// If the variable is defined as public in the current scope, this returns only that single
    /// location. Otherwise, it falls back to `pub_var_defs` to return all known public definitions
    /// across classes.
    fn get_pub_variable_symbol(
        &self,
        symbol_name: &str,
        url: &Url,
        point: Point,
    ) -> Vec<(Url, Range)> {
        start_of_function("ProjectData", "get_pub_variable_symbol");
        let mut locations = Vec::new();
        let var_in_scope = self.get_pub_var_symbol_from_current_scope(symbol_name, url, point);
        if var_in_scope.is_some() {
            locations.push(var_in_scope.unwrap());
            successful_exit("ProjectData", "get_pub_variable_symbol");
            return locations;
        }
        let Some(symbol_defs_by_class) = self.pub_var_defs.get(symbol_name) else {
            eprintln!("Couldn't find hashmap associated with given symbol name: {:?}. Pub Var Defs is: \n {:?} \n\n", symbol_name, self.pub_var_defs);
            generic_exit_statements("ProjectData", "get_pub_variable_symbol");
            return locations;
        };
        for (class_name, symbols_defs) in symbol_defs_by_class {
            let Some(class_symbol_id) = self.class_defs.get(class_name) else {
                eprintln!(
                    "Couldn't find class symbol id in class defs for class named {:?}",
                    class_name
                );
                generic_skipping_statements(
                    "get_pub_variable_symbol",
                    class_name.as_str(),
                    "Class",
                );
                continue;
            };
            for def in symbols_defs {
                let Some(symbol) = self.global_semantic_model.get_variable_symbol(
                    class_symbol_id,
                    def.0,
                    class_name,
                ) else {
                    generic_skipping_statements(
                        "get_pub_variable_symbol",
                        class_name.as_str(),
                        "Class Symbol for class named",
                    );
                    continue;
                };
                locations.push((symbol.url.clone(), symbol.location));
            }
        }
        successful_exit("ProjectData", "get_pub_variable_symbol");
        locations
    }

    /// Find the definition location(s) for a variable at a given cursor point in a method.
    ///
    /// Determines whether the symbol should be treated as private (procedure block + not explicitly
    /// declared public in the method) or public. Private symbols are resolved via the document's
    /// `ScopeTree`; public symbols are resolved using the project-wide public variable index.
    pub fn get_variable_symbol_location(
        &self,
        url: Url,
        point: Point,
        symbol_name: String,
        method_name: String,
    ) -> Vec<(Url, Range)> {
        start_of_function("ProjectData", "get_variable_symbol_location");
        let mut locations = Vec::new();
        let Some(document) = self.get_document(&url) else {
            generic_exit_statements("ProjectData", "get_variable_symbol_location");
            return locations;
        };

        let class_id = match document.class_id {
            Some(id) => id,
            None => {
                eprintln!(
                    "Error: failed to get class id from document for file {:?}",
                    url.path()
                );
                generic_exit_statements("ProjectData", "get_variable_symbol_location");
                return locations;
            }
        };

        let Some(class) = self.global_semantic_model.get_class(class_id.0) else {
            generic_exit_statements("ProjectData", "get_variable_symbol_location");
            return locations;
        };

        let class_name = class.name.as_str();

        let mut is_procedure_block = true;
        let mut symbol_is_public = false;

        if let Some(public_method_id) = class.get_public_method_id(&method_name) {
            let Some(method) = self.global_semantic_model.get_method(
                class_id,
                class.name.as_str(),
                public_method_id.0,
            ) else {
                generic_exit_statements("ProjectData", "get_variable_symbol_location");
                return locations;
            };
            let method_is_procedure_block = if method.is_procedure_block.is_some() {
                method.is_procedure_block.unwrap()
            } else if class.is_procedure_block.is_some() {
                class.is_procedure_block.unwrap()
            } else {
                true
            };
            is_procedure_block = method_is_procedure_block;
            if method.public_variables_declared.contains(&symbol_name) {
                symbol_is_public = true;
            }
        } else if let Some(private_method_id) = class.get_private_method_id(&method_name) {
            let Some(local_semantic_model_id) = document.local_semantic_model_id else {
                eprintln!(
                    "Error: document for file {:?} doesn't have local semantic model id",
                    url.path()
                );
                generic_exit_statements("ProjectData", "get_variable_symbol_location");
                return locations;
            };
            let Some(local_semantic_model) = self
                .global_semantic_model
                .get_local_semantic(local_semantic_model_id)
            else {
                generic_exit_statements("ProjectData", "get_variable_symbol_location");
                return locations;
            };
            let Some(method) = local_semantic_model.get_method(private_method_id.0) else {
                generic_exit_statements("ProjectData", "get_variable_symbol_location");
                return locations;
            };
            let method_is_procedure_block = if method.is_procedure_block.is_some() {
                method.is_procedure_block.unwrap()
            } else if class.is_procedure_block.is_some() {
                class.is_procedure_block.unwrap()
            } else {
                true
            };
            is_procedure_block = method_is_procedure_block;

            if method.public_variables_declared.contains(&symbol_name) {
                symbol_is_public = true;
            }
        } else {
            eprintln!(
                "Error: Failed to find a method named {:?} in class: {:?}",
                method_name, class_name
            );
            generic_exit_statements("ProjectData", "get_variable_symbol_location");
            return locations;
        }

        if is_procedure_block && !symbol_is_public {
            // variable is private
            let Some(range) = document
                .scope_tree
                .get_variable_definition(point, symbol_name.as_str())
            else {
                generic_exit_statements("ProjectData", "get_variable_symbol_location");
                return locations;
            };
            locations.push((url.clone(), range));
        } else {
            locations = self.get_pub_variable_symbol(&symbol_name, &url, point);
        }
        eprintln!("Leaving ProjectData function: get_variable_symbol_location.. the number of potential definitions for symbol named: {:?} in method {:?} are: \n {:?}", symbol_name, method_name, locations.len());
        eprintln!("------------------------");
        eprintln!();
        locations
    }

    /// Return locations of methods that override a given public method.
    ///
    /// Looks up the current document's class, confirms `method_name` is a public method, then uses
    /// `override_index.overridden_by` to find overriding methods (public or private) in subclasses.
    ///
    /// Each returned `(Url, Range)` points to the overriding method's definition location.
    pub fn get_method_overrides(&self, url: Url, method_name: String) -> Vec<(Url, Range)> {
        eprintln!("------------------------");
        eprintln!(
            "In ProjectData function: get_method_overrides.. for method: {:?}",
            method_name
        );
        eprintln!();
        let mut locations = Vec::new();
        let method_name_str = method_name.as_str();
        let Some(document) = self.get_document(&url) else {
            eprintln!("Aborting function early");
            print_statements_exit_method_overrides_fn(
                method_name_str,
                "Unknown, couldn't access document",
                locations.clone(),
            );
            return locations;
        };

        let class_id = match document.class_id {
            Some(id) => id,
            None => {
                eprintln!(
                    "Error: failed to get class id from document {:?}, aborting function",
                    document
                );
                print_statements_exit_method_overrides_fn(
                    method_name_str,
                    "Unknown, couldn't access document class id",
                    locations.clone(),
                );
                return locations;
            }
        };

        let Some(class) = self.global_semantic_model.get_class(class_id.0) else {
            eprintln!("Aborting function early");
            print_statements_exit_method_overrides_fn(
                method_name_str,
                "Unknown, class DNE",
                locations.clone(),
            );
            return locations;
        };

        let Some(&public_method_id) = class.get_public_method_id(method_name.as_str()) else {
            eprintln!("Aborting function early, method either DNE or is not public");
            print_statements_exit_method_overrides_fn(
                method_name_str,
                class.name.as_str(),
                locations.clone(),
            );
            return locations;
        };

        let method_ref = PublicMethodRef {
            class: class_id,
            id: public_method_id,
        };

        // ---- overridden-by list ----
        let overrides = match self.override_index.overridden_by.get(&method_ref) {
            Some(v) => v,
            None => {
                print_statements_exit_method_overrides_fn(
                    method_name_str,
                    class.name.as_str(),
                    locations.clone(),
                );
                return locations;
            }
        };

        for override_method_ref in overrides {
            let Some(class) = self
                .global_semantic_model
                .get_class(override_method_ref.class.0)
            else {
                generic_skipping_statements(
                    "get_method_overrides",
                    method_name_str,
                    "Override method ref of the method named",
                );
                continue;
            };

            let cls_name = &class.name;

            let overriding_subclass_class_symbol_id = match self.class_defs.get(cls_name).copied() {
                Some(id) => id,
                None => {
                    generic_skipping_statements(
                        "get_method_overrides",
                        method_name_str,
                        "Override method ref of the method named",
                    );
                    continue;
                }
            };

            if let Some(_) = override_method_ref.pub_id {
                let Some(sym) = self.get_public_method_symbol(
                    cls_name.as_str(),
                    method_name.as_str(),
                    overriding_subclass_class_symbol_id,
                ) else {
                    generic_skipping_statements(
                        "get_method_overrides",
                        method_name_str,
                        "Override method ref of the method named",
                    );
                    continue;
                };
                locations.push((sym.url.clone(), sym.location));
            } else {
                // method that is overriding is private.
                let Some(overriding_cls_symbol) = self
                    .global_semantic_model
                    .get_class_symbol(overriding_subclass_class_symbol_id.0, cls_name.as_str())
                else {
                    generic_skipping_statements(
                        "get_method_overrides",
                        method_name_str,
                        "Override method ref of the method named",
                    );
                    continue;
                };
                let cls_url = &overriding_cls_symbol.url;

                let doc = match self.get_document(cls_url) {
                    Some(d) => d,
                    None => continue,
                };

                let Some(sym) = doc
                    .scope_tree
                    .get_private_method_symbol(method_name.as_str())
                else {
                    generic_skipping_statements(
                        "get_method_overrides",
                        method_name_str,
                        "Override method ref of the method named",
                    );
                    continue;
                };
                locations.push((cls_url.clone(), sym.location));
            }
        }
        successful_exit("ProjectData", "get_method_overrides");
        locations
    }
}

impl ProjectState {
    /// Create a new `ProjectState` with default configuration and empty indexing state.
    ///
    /// Initializes shared parsers, an empty `ProjectData` store, and leaves `project_root_path`
    /// unset (expected to be populated during LSP initialization).
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

    /// Handle an LSP `textDocument/didOpen` by parsing and committing the document.
    ///
    /// Parses the text with the appropriate Tree-sitter grammar, derives the class name for `.cls`
    /// files, then updates project state inside a single write lock:
    /// - Adds the document if new, or updates it if contents/type changed
    /// - Rebuilds inheritance/override/call/variable indexes for affected state
    pub fn handle_document_opened(
        &self,
        url: Url,
        text: String,
        file_type: FileType,
        version: i32,
    ) {
        start_of_function("ProjectState", "handle_document_opened");
        // Parse OUTSIDE lock
        let tree = if file_type == FileType::Cls {
            match self.parsers.cls.lock().parse(&text, None) {
                Some(t) => t,
                None => {
                    eprintln!("parse failed for cls file with content: {}", text);
                    generic_exit_statements("ProjectState", "handle_document_opened");
                    return;
                }
            }
        } else {
            match self.parsers.routine.lock().parse(&text, None) {
                Some(t) => t,
                None => {
                    eprintln!("parse failed for routine file with content: {}", text);
                    generic_exit_statements("ProjectState", "handle_document_opened");
                    return;
                }
            }
        };
        if file_type != FileType::Cls {
            eprintln!("file type is unimplemented {:?}", file_type);
            generic_exit_statements("ProjectState", "handle_document_opened");
            return;
        }
        let Some(class_name) = get_class_name_from_root(&text, tree.root_node()) else {
            generic_exit_statements("ProjectState", "handle_document_opened");
            return;
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
                data.build_inheritance_and_variables(Some(url), Vec::new());
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

        successful_exit("ProjectState", "handle_document_opened");
    }

    /// Wrapper to read document info from the inner `ProjectData`.
    pub fn get_document_info(&self, url: &Url) -> Option<(FileType, String, i32, Tree)> {
        self.data.read().get_document_info(url)
    }

    /// Wrapper to update a document inside the inner `ProjectData`
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

    /// Return the project root path, if initialized.
    pub fn root_path(&self) -> Option<&std::path::Path> {
        self.project_root_path.get().and_then(|o| o.as_deref())
    }
}
