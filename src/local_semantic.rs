use crate::parse_structures::*;

/*
The relative dot syntax (..) provides a mechanism for referencing a method or property in the current context. T
he context for an instance method or a property is the current instance; the context for a class method is the class in which the method is implemented.
 You cannot use relative dot syntax in a class method to reference properties or instance methods, because these require the instance context.
*/

impl LocalSemanticModel {
    pub fn new() -> Self {
        Self {
            methods: Vec::new(),
            properties: Vec::new(),
            variables: Vec::new(),
        }
    }

    // pub fn get_method(&self, method_id: PrivateMethodId) -> Option<&Method> {
    //     self.methods.get(method_id.0)
    // }
    //
    // pub fn get_method_mut(&mut self, method_id: PrivateMethodId) -> Option<&mut Method> {
    //     self.methods.get_mut(method_id.0)
    // }

    pub(crate) fn new_variable(&mut self, variable: Variable) -> PrivateVarId {
        let id = PrivateVarId(self.variables.len());
        self.variables.push(variable);
        id
    }

    pub(crate) fn new_method(&mut self, method: Method) -> PrivateMethodId {
        let id = PrivateMethodId(self.methods.len());
        self.methods.push(method);
        id
    }
}
