use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::Range;
use crate::scope_tree::ScopeId;
use tower_lsp::lsp_types::{Url};
use crate::parse_structures::{Class, Method, MethodId, Variable, VarId, ParameterId, SymbolId, SymbolKind, Symbol, ClassProperty, ClassParameter, Var, PropertyId, MethodType};
use parking_lot::RwLock;

#[derive(Copy, Clone, Debug)]
enum VarVisibility {
    Public,
    Private,
}

// TODO: globally, can map class name -> url, and then url -> semantic model
pub struct LocalSemanticModelWrapper (Arc<RwLock<LocalSemanticModel>>);

pub struct GlobalSemanticModelWrapper (Arc<RwLock<GlobalSemanticModel>>);

pub struct GlobalVarRef {
    pub url: Url,
    pub var_id: VarId,
}
pub struct GlobalSemanticModel {
    // TODO: might want to store undefined ranges for variables
    pub public_local_vars: HashMap<String, Vec<GlobalVarRef>>, // most useful for NotProcedure Blocks; I stored Vec<Variable>, because I am thinking about how it could be set to diff types in diff methods?
    pub global_variables: HashMap<String, Vec<GlobalVarRef>>,
    pub classes: HashMap<String,Url>,
}
pub struct LocalSemanticModel {
    pub class: Class,
    pub methods: Vec<Method>,
    pub properties: Vec<ClassProperty>,
    pub class_parameters: Vec<ClassParameter>,
    pub symbols: Vec<Symbol>,
    pub vars: Vec<Var>,
}

impl LocalSemanticModelWrapper {
    pub fn new(semantic_model: LocalSemanticModel) -> Self {
        Self(Arc::new(RwLock::new(semantic_model)))
    }
}

impl GlobalSemanticModelWrapper {
    pub fn new(semantic_model: GlobalSemanticModel) -> Self {
        Self(Arc::new(RwLock::new(semantic_model)))
    }
}

impl GlobalSemanticModel {
    pub fn new() -> Self {

        Self {
            public_local_vars: HashMap::new(),
            global_variables: HashMap::new(),
            classes: HashMap::new(),
        }
    }

    pub fn new_public_local_var(&mut self, url: Url, var_id: VarId, var_name: String) {
        let global_ref = GlobalVarRef { url, var_id };
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
    pub fn new(class:Class) -> Self {
        Self {
            class,
            methods: Vec::new(),
            properties: Vec::new(),
            class_parameters: Vec::new(),
            symbols: Vec::new(),
            vars: Vec::new(),
        }
    }

    pub fn get_instance_method(&self, method_name: String) -> Option<&Method> {
        let method_id = self.class.instance_methods.get(&method_name)?;
        self.methods.get(method_id.0)
    }

    pub fn get_instance_method_mut(&mut self, method_name: String) -> Option<&mut Method> {
        let method_id = self.class.instance_methods.get(&method_name)?;
        self.methods.get_mut(method_id.0)
    }

    pub fn get_class_method(&self, method_name: String) -> Option<&Method> {
        let method_id = self.class.class_methods.get(&method_name)?;
        self.methods.get(method_id.0)
    }

    pub fn get_class_method_mut(&mut self, method_name: String) -> Option<&mut Method> {
        let method_id = self.class.class_methods.get(&method_name)?;
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
            range,
            scope,
            references: Vec::new(),
        });
        id
    }

    pub fn new_var(&mut self, var: Var) -> VarId {
        let id = VarId(self.vars.len());
        self.vars.push(var);
        id
    }

    fn attach_var_to_method(
        &mut self,
        method_name: String,
        var_name: &str,
        var_id: VarId,
        visibility: VarVisibility,
    ) {
        let method = self.get_method_mut(method_name)
            .unwrap_or_else(|| panic!("Method '{}' not found", method_name));

        let map = match visibility {
            VarVisibility::Public => &mut method.pub_vars,
            VarVisibility::Private => &mut method.priv_vars,
        };

        if map.contains_key(var_name) {
            // TODO: can turn this into a diagnostic instead of panic
            panic!("Variable '{}' already exists in method '{}'", var_name, method_name);
        }

        map.insert(var_name.to_string(), var_id);
    }

    fn get_var_name(&self, var: &Var) -> String {
        let var_name = match var {
            Var::Variable(var) => {var.name.clone()},
            Var::ClassInstance(class_instance) => {class_instance.name.clone()},
            Var::MethodParameter(method_param) => {method_param.name.clone()},
        };
        var_name
    }

    pub fn new_public_var(&mut self, method_name: String, var: Var) -> VarId {
        let var_name = self.get_var_name(&var);
        let var_id = self.new_var(var);
        self.attach_var_to_method(method_name, &var_name, var_id, VarVisibility::Public);
        var_id
    }

    pub fn new_private_var(&mut self, method_name: String, var: Var) -> VarId {
        let var_name = self.get_var_name(&var);
        let var_id = self.new_var(var);
        self.attach_var_to_method(method_name, &var_name, var_id, VarVisibility::Private);
        var_id
    }

    pub fn new_class_parameter(&mut self, parameter: ClassParameter) -> ParameterId {
        let id = ParameterId(self.class_parameters.len());
        let param_name = parameter.name.clone();
        if self.class.parameters.contains_key(&param_name) {
            panic!("Property {} already exists", param_name);
        }
        else {
            self.class_parameters.push(parameter);
            self.class.parameters.insert(param_name.clone(),id);
        }
        id
    }

    pub fn new_class_property(&mut self, property: ClassProperty) -> PropertyId {
        let id = PropertyId(self.properties.len());
        let property_name = property.name.clone();
        if self.class.properties.contains_key(&property_name) {
            panic!("Property {} already exists", property_name);
        }
        else {
            self.properties.push(property);
            self.class.properties.insert(property_name.clone(),id);
        }
        id
    }

    // TODO: might want to return a result rather than ID, don't rly wanna return ID if the method doesn't actually get pushed
    pub fn new_method(
        &mut self,
        method: Method,
    ) -> MethodId {
        let id = MethodId(self.methods.len());
        let method_name = method.name.clone();
        let method_type = method.method_type.clone();
        match method_type {
            MethodType::InstanceMethod => {
                if self.class.instance_methods.contains_key(&method_name) {
                    panic!("Method '{}' already exists", method_name);
                }
                else {
                    self.methods.push(method);
                    self.class.instance_methods.insert(method_name, id);
                }
            },
            MethodType::ClassMethod => {
                if self.class.class_methods.contains_key(&method_name) {
                    panic!("Method '{}' already exists", method_name);
                }
                else {
                    self.methods.push(method);
                    self.class.class_methods.insert(method_name, id);
                }
            }
        }
        id
    }
}
