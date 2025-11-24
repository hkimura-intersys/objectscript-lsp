use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    enable_snippets: Option<bool>,
    enable_formatting: Option<bool>,
    enable_lint: Option<bool>,
    enable_strict_mode: Option<bool>,
}

impl Config {

    pub fn is_strict_mode_enabled(&self) -> bool {
        self.enable_strict_mode.unwrap_or(true)
    }
    pub fn are_snippets_enabled(&self) -> bool {
        self.enable_snippets.unwrap_or(true)
    }

    pub fn is_formatting_enabled(&self) -> bool {
        self.enable_formatting.unwrap_or(true)
    }

    pub fn is_lint_enabled(&self) -> bool {
        self.enable_lint.unwrap_or(true)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable_formatting: Some(true),
            enable_lint: Some(true),
            enable_snippets: Some(true),
        }
    }
}
