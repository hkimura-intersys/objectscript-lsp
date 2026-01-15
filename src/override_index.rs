use crate::parse_structures::*;
use std::collections::HashMap;

impl OverrideIndex {
    pub fn new() -> Self {
        Self {
            effective_public_methods: HashMap::new(),
            overrides: HashMap::new(),
            overridden_by: HashMap::new(),
        }
    }
}

#[derive(Default, Debug)]
pub struct OverrideIndex {
    /// For completion / resolution: after inheritance + overrides,
    /// what method id does a class see for each public method name?
    pub effective_public_methods: HashMap<ClassId, HashMap<String, PublicMethodRef>>,

    /// child -> base (the inherited methoda it replaced)
    /// A private method can overwrite a public method, but only public methods can be overwritten.
    pub overrides: HashMap<MethodRef, PublicMethodRef>,

    /// base -> children
    pub overridden_by: HashMap<PublicMethodRef, Vec<MethodRef>>,
}
