# tower-lsp-objectscript

GOAL: Use tower-lsp to create a language server for objectscript. Perhaps use the tree-sitter implementation as part of the language server implementation.

BENEFITS:
- this gives us a single, fast, cross-editor server (rather than just being compatible with vscode)
- using tree-sitter within this allows for extremely fast, incremental parsing with low memory
