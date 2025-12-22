use crate::parse_structures::{LocalSemanticModel,
                              GlobalSemanticModel,
                              LocalSemanticModelId,
                              Class,
                              Variable,
                              VarId,
                              ClassId,
                              MethodId,
                              Method};
use crate::scope_structures::{GlobalSymbol, GlobalSymbolKind, Symbol, SymbolId};
use tree_sitter::Range;
use tower_lsp::lsp_types::Url;

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

    pub fn new_local_semantic(&mut self, local_semantic: LocalSemanticModel) -> LocalSemanticModelId {
        let id = LocalSemanticModelId(self.private.len());
        self.private.push(local_semantic);
        id
    }

    pub fn new_symbol(&mut self, name: String, kind: GlobalSymbolKind, range: Range, url: Url) -> SymbolId {
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
}
