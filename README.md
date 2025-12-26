# OBJECTSCRIPT LANGUAGE SERVER USING TOWER_LS
design a language server that will be usable from any editor that has support for tree-sitter grammars (not solely limited to VSCode)
Features include:
- diagnostics (find undefined variables, find refs for each symbol)
- inheritance (for a superclass, find which methods are overridden and by which subclass(es))  (COMPLETED)
- formatting: turn dotted statements into their corresponding block format  
- go-to definitions


### Completed 
1. Initial Build:
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

2. The Second Iteration Build: 
   - finds the direct inherited classes (those specified; indirect inherited classes would be classes that your inherited class inherits themselves)
   - finds what class keywords are inherited for a given class
   - builds the override index, which tracks the following: 
     - effective_public_methods - for a given class, what methods are available (this includes inherited methods).  
     - overrides - for a subclass, tracks which methods it overwrote (and which superclass).  
     - overridden_by - for a superclass, tracks which subclasses overwrote which methods.  
     - method_owner - for a method, tracks the class that is the original owner of the method.
   - builds variables and method arguments, keeping track of return type (for method args), and the var type (what is the variable actually being set to).
   - handles building symbols and global symbols for variables (private vs public), and stores the symbol in the workspace (project state) if public and in the corresponding scope tree if private.


### TODO
1. Strict mode (not yet done): 
    - need to declare vars with dim (look up how they did that with intersystems studio 
    - can give a warning for types that aren't expected
3. AI assistant
5. properties, parameters, relationships, foreignkey, query, index, xdata, trigger, projection, storage 
