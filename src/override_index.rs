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
