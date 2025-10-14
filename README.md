# tower-lsp-objectscript

GOAL: Use tower-lsp to create a language server for objectscript. Use tree-sitter as part of the implementation, can use the tree-sitter stuff to find nodes (for go-to definition) or for parameter finding

- may want to look into using local variables per grammar (as each may vary in how to get the nodes)
BENEFITS:
- this gives us a single, fast, cross-editor server (rather than just being compatible with vscode)
- using tree-sitter within this allows for extremely fast, incremental parsing with low memory


RUST FEATURES NOT IMPLEMENTED IN ZED:
- hover
- go to definition
- In rust rover, if I spell a dependency wrong, or put one that can't be found, it highlights it, showing me where the error is. In Zed, it looks like nothing is wrong.
