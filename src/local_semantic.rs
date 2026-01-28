use crate::common::{generic_exit_statements, start_of_function, successful_exit};
use crate::parse_structures::{Variable, PrivateVarId, Method, PrivateMethodId, ClassProperty};
impl LocalSemanticModel {
    /// Creates a new, empty `LocalSemanticModel` with `active` set to `true`.
    pub fn new() -> Self {
        Self {
            methods: Vec::new(),
            properties: Vec::new(),
            variables: Vec::new(),
            active: true,
        }
    }

    /// Clears all stored methods/properties/variables and marks the model as inactive.
    pub fn clear(&mut self) {
        self.methods.clear();
        self.properties.clear();
        self.variables.clear();
        self.active = false;
    }

    /// Adds a new private/local variable to this model and returns its `PrivateVarId`.
    ///
    /// The returned id is the index of the variable in the internal `variables` vector.
    pub(crate) fn new_variable(&mut self, variable: Variable) -> PrivateVarId {
        start_of_function("LocalSemanticModel", "new_variable");
        let id = PrivateVarId(self.variables.len());
        eprintln!(
            "Info: Adding variable {:?} to local semantic model",
            variable.name.as_str()
        );
        self.variables.push(variable);
        successful_exit("LocalSemanticModel", "new_variable");
        id
    }

    /// Adds a new private/local method to this model and returns its `PrivateMethodId`.
    ///
    /// The returned id is the index of the method in the internal `methods` vector.
    pub(crate) fn new_method(&mut self, method: Method) -> PrivateMethodId {
        start_of_function("LocalSemanticModel", "new_method");
        let id = PrivateMethodId(self.methods.len());
        eprintln!(
            "Info: Adding Method {:?} to local semantic model",
            method.name.as_str()
        );
        self.methods.push(method);
        successful_exit("LocalSemanticModel", "new_method");
        id
    }

    /// Returns an immutable reference to the private/local method at `private_method_id`.
    ///
    /// Logs a warning and returns `None` if the index is out of bounds.
    pub(crate) fn get_method(&self, private_method_id: usize) -> Option<&Method> {
        start_of_function("LocalSemanticModel", "get_method");
        let result = self.methods.get(private_method_id);
        match result {
            None => {
                eprintln!("Warning: Failed to get method from local semantic model: Index {:?} out of bounds for methods vector: {sep} {:?} {sep}", private_method_id, self.methods, sep= "\n");
                generic_exit_statements("LocalSemanticModel", "get_method");
                result
            }
            Some(_) => {
                successful_exit("LocalSemanticModel", "get_method");
                result
            }
        }
    }

    /// Returns a mutable reference to the private/local method at `index`.
    ///
    /// Logs a warning and returns `None` if the index is out of bounds.
    pub(crate) fn get_method_mut(&mut self, index: usize) -> Option<&mut Method> {
        start_of_function("LocalSemanticModel", "get_method_mut");
        if index > self.methods.len() {
            eprintln!("Warning: Failed to get method from local semantic model: Index {:?} out of bounds for methods vector of len: {sep} {:?} {sep}", index, self.methods.len(), sep= "\n");
            generic_exit_statements("LocalSemanticModel", "get_method_mut");
        }
        successful_exit("LocalSemanticModel", "get_method_mut");
        self.methods.get_mut(index)
    }
}

/// Per-document private semantic state (methods, properties, variables).
///
/// This is used for private members that should not be shared across classes globally.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSemanticModel {
    pub methods: Vec<Method>,
    pub properties: Vec<ClassProperty>,
    pub variables: Vec<Variable>,
    pub active: bool,
}
