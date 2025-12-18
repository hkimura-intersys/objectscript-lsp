use std::collections::HashMap;
use tree_sitter::{Node, Range};
use crate::scope_tree::ScopeId;
use tower_lsp::lsp_types::{Url};
use crate::parse_structures::*;
use serde_json::Value;
#[derive(Copy, Clone, Debug)]
enum VarVisibility {
    Public,
    Private,
}

pub fn get_keyword(keyword_type: &str, filter:&str) -> String {
    let json = tree_sitter_objectscript::OBJECTSCRIPT_NODE_TYPES; // &'static str
    let v: Value = serde_json::from_str(json).expect("invalid node-types.json");


    // node-types.json is an array of objects
    let arr = v.as_array().expect("node-types.json must be a JSON array");

    // find the object with "type": "class_keyword"
    let keyword = arr
        .iter()
        .find(|obj| obj.get("type").and_then(Value::as_str) == Some(keyword_type));

    if let Some(obj) = keyword {
        if let Some(types) = obj
            .get("children")
            .and_then(|c| c.get("types"))
            .and_then(Value::as_array)
        {
            for t in types {
                if let Some(ty) = t.get("type").and_then(Value::as_str) {
                    if ty.contains(filter) {
                        return ty.to_string();
                    }
                }
            }
        }
    }
    "".to_string()
}

pub fn get_node_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect::<Vec<Node>>()
}

#[derive(Clone, Debug)]
pub struct GlobalVarRef {
    pub url: Url,
    pub location: Range,
    pub var_id: VarId,
}

#[derive(Clone, Debug)]
pub struct GlobalSemanticModel {
    // TODO: might want to store undefined ranges for variables
    pub public_local_vars: HashMap<String, Vec<GlobalVarRef>>, // most useful for NotProcedure Blocks; I stored Vec<Variable>, because I am thinking about how it could be set to diff types in diff methods?
    pub global_variables: HashMap<String, Vec<GlobalVarRef>>,
    // need this in the case that a subclass is parsed before a class
    pub subclasses: HashMap<String, Vec<String>>,
    pub classes: HashMap<String,Url>,
}

#[derive(Clone, Debug)]
pub struct LocalSemanticModel {
    pub class: Class,
    pub methods: Vec<Method>,
    pub properties: Vec<ClassProperty>,
    pub class_parameters: Vec<ClassParameter>,
    pub symbols: Vec<Symbol>,
    pub vars: Vec<Variable>,
    content: String,
}

impl GlobalSemanticModel {
    pub fn new() -> Self {

        Self {
            public_local_vars: HashMap::new(),
            global_variables: HashMap::new(),
            classes: HashMap::new(),
            subclasses: HashMap::new(),
        }
    }

    pub fn add_subclass(&mut self, class_name: String, subclass_name: String) {
        if self.subclasses.contains_key(&class_name) {
            let refs = self.subclasses.get_mut(&class_name).unwrap();
            refs.push(subclass_name);
        }
        else {
            self.subclasses.insert(class_name, vec![subclass_name]);
        }
    }

    pub fn new_public_local_var(&mut self, url: Url, location: Range, var_id: VarId, var_name: String) {
        let global_ref = GlobalVarRef { url, location, var_id };
        if self.public_local_vars.contains_key(&var_name) {
            let refs = self.public_local_vars.get_mut(&var_name).unwrap();
            refs.push(global_ref);
        }
        else {
            self.public_local_vars.insert(var_name, vec![global_ref]);
        }
    }
}
impl LocalSemanticModel {
    pub fn new(class:Class, content:String ) -> Self {
        Self {
            class,
            methods: Vec::new(),
            properties: Vec::new(),
            class_parameters: Vec::new(),
            symbols: Vec::new(),
            vars: Vec::new(),
            content
        }
    }

    pub fn get_method(&self, method_name: String) -> Option<&Method> {
        let method_id = self.class.methods.get(&method_name)?;
        self.methods.get(method_id.0)
    }

    pub fn get_method_mut(&mut self, method_name: String) -> Option<&mut Method> {
        let method_id = self.class.methods.get(&method_name)?;
        self.methods.get_mut(method_id.0)
    }
    pub fn get_class(&self) -> Class {
        self.class.clone()
    }

    pub fn new_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        range: Range,
        scope: ScopeId,
    ) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol {
            name,
            kind,
            location: range,
            scope,
            references: Vec::new(),
        });
        id
    }

    /// Given the class_keywords node, parses out the
    /// keywords to find if there is one for ProcedureBlock
    /// or for Language. If so, adjusts the class structure accordingly.
    fn handle_class_keywords(&mut self,node:Node) {
        let class_keywords_children = get_node_children(node.clone());
        let procedure_block = get_keyword("class_keyword", "procedure");
        let language_keyword = get_keyword("class_keyword", "language");
        let inheritance_keyword = get_keyword("class_keyword", "inheritance");
        // each node here is a class_keyword
        for node in class_keywords_children.iter() {
            let keyword = node.named_child(0).unwrap();
            if keyword.kind() == procedure_block {
                if keyword.named_child(0).unwrap().kind() == "keyword_not" {
                    self.class.is_procedure_block = Some(false);
                }
                else {
                    self.class.is_procedure_block = Some(true);
                }
            }
            else if keyword.kind() == language_keyword {
                if self.content[keyword.named_child(1).unwrap().byte_range()].to_string().to_lowercase() == "tsql" {
                    self.class.default_language = Some(Language::TSql);
                }
                else if self.content[keyword.named_child(1).unwrap().byte_range()].to_string().to_lowercase() == "objectscript" {
                    self.class.default_language = Some(Language::Objectscript);
                }
                else {
                    println!("KEYWORD {:?}", self.content[keyword.byte_range()].to_string());
                    println!("LANGUAGE SPECIFIED IS NOT ALLOWED {:?}",self.content[keyword.named_child(1).unwrap().byte_range()].to_string().to_lowercase())
                }
            }
            else if keyword.kind() == inheritance_keyword {
                if self.content[keyword.named_child(1).unwrap().byte_range()].to_string().to_lowercase() == "right" {
                    self.class.inheritance_direction = "right".to_string();
                }
            }
        }
    }

    /// given the class_extends node, adds any classes
    /// that are extended by this class
    fn add_inherited_classes(&mut self, node: Node) {
        let children = get_node_children(node.clone());
        // skip first node, which is just keyword_extends
        for child in children[1..].iter() {
             self.class.inherited_classes.push(self.content[child.byte_range()].to_string());
        }
    }

    /// Takes in a class_definition node and walks it to update the Class Struct. Stores semantic information
    /// for class keywords, inherited classes, class properties, class parameters.
    /// TODO: relationships, foreignkey, query, method/classmethod, index, trigger,  xdata, projection, storage
    pub(crate) fn cls_build_symbol_table(&mut self, node: Node) {
        let class_children = get_node_children(node);
        // skip keyword_class and class_name
        for node in class_children[2..].iter() {
            match node.kind() {
                "class_keywords" => {
                    self.handle_class_keywords(node.clone());
                },
                "class_extends" => {
                    self.add_inherited_classes(node.clone());
                },
                _ => { println!("Unimplemented class child {}", node.kind()) }
            }
        }
    }
}
