use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub(crate) enable_snippets: bool,
    pub(crate) enable_formatting: bool,
    enable_lint: bool,
    enable_strict_mode: bool,
}

impl Config {
    pub fn is_strict_mode_enabled(&self) -> bool {
        self.enable_strict_mode
    }
    pub fn are_snippets_enabled(&self) -> bool {
        self.enable_snippets
    }

    pub fn is_formatting_enabled(&self) -> bool {
        self.enable_formatting
    }

    pub fn is_lint_enabled(&self) -> bool {
        self.enable_lint
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable_strict_mode: true,
            enable_formatting: true,
            enable_lint: true,
            enable_snippets: true,
        }
    }
}
