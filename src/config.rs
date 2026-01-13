use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub(crate) enable_snippets: bool,
    pub(crate) enable_formatting: bool,
    enable_lint: bool,
    enable_strict_mode: bool,
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
