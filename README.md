# OBJECTSCRIPT LANGUAGE SERVER USING TOWER_LS
design a language server that will be usable from any editor that has support for tree-sitter grammars (not solely limited to VSCode)
Features include:
- diagnostics (find undefined variables, find refs for each symbol)
- inheritance (for a superclass, find which methods are overridden and by which subclass(es))  (COMPLETED)
- formatting: turn dotted statements into their corresponding block format  
- go-to definitions


### Completed 
1. Initial Build and Inheritance (part of second iteration):
   the initial build (DONE):
   - sets the procedure block default setting
   - sets the default language
   - sets the inheritance direction if specified
   - builds the method struct for each method → initial_build_method()
   - sets the language, procedure block setting, codemode, privacy, and declared public variables for the method
   - sets the return type of the method
   - creates a symbol for the class and each method, adding private symbols to their corresponding Scope in the scope tree and public symbols to the global_semantic_model.defs field.

once the initial build finishes, the class struct and public methods are added to the global semantic model, while private methods are added to the local semantic model. Additionally, 
symbols are created for each of these, with the private ones being added to the local semantic models and the public ones added to global semantic model.

2. inheritance: (for a superclass, find which methods are overridden and by which subclass(es))
2. ScopeTree per file and global defs (for go-to definitions)

### TODO
1. Strict mode (not yet done): 
    - need to declare vars with dim (look up how they did that with intersystems studio 
    - can give a warning for types that aren't expected
2. semantics for variables 
3. AI assistant
4. method arguments and variables
5. properties, parameters, relationships, foreignkey, query, index, xdata, trigger, projection, storage 
