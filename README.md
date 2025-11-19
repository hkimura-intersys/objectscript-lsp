# OBJECTSCRIPT LANGUAGE SERVER USING TOWER_LS

## Overview
This implementation uses the tower_ls rust trait to implement an objectscript language server. 

The languageServer trait defines the capabilities of our languageServer. The client represents any LSP compliant IDE.
Note that all things related to the client are sent through the IDE (all client capabilities).

### COMMUNICATION BETWEEN LS AND LS CLIENT
There are three types of messages: 
1. Request (sender -> receiver): receiver must respond with response
2. Response - must be sent as a result for a particular request
3. Notification - this is basically an event 

The LS Client and LS communicate upon certain actions/events (which have been formalized in a set of documented features by the LSP). A capability is the 
ability to provide support for a particular feature/event/action (both the LS and LS Client have a set of capabilities).

Client Mandatory Capabilities:
1. didOpen - notif sent from client to server to signal the opening of a new doc
2. didChange - notif sent client to server to signal a change in the doc 
3. didClose - notif sent from client to server to signal the closure of a doc

### Potential Server Capabilities
- TextDocumentSyncCapability - tells the LSP client how the server wants to receive document synchronization notifications (like didOpen, didChange, didClose, didSave)
  - This controls when the server wants to be notified about document changes, how the document changes should be sent (full document content vs incremental changes), 
  and what document lifecycle events the server wants to receive
  - Since our implementation uses an AST, we want to use `TextDocumentSyncKind::INCREMENTAL`. When you use this, the client
  sends `TextDocumentContentChangeEvent` objects that contain the range of the text that changed, the new text to insert,
  and the length of text that was replaced. This maps directly to tree-sitter's incremental parsing API, which accepts edit
  info (start position, old end position, new end position) to update the syntax tree efficiently.
   
- `position_encoding` - a server capability that tells the LSP client which character encoding the server uses to calculate
positions in text documents. (Default: UTF-16)
  - Tree-sitter must use UTF-8 
  - Due to this, we need to be able to convert between UTF-8 and UTF-16, as some IDEs only support UTF-16 (Vscode, and maybe zed too)
  - So, position encoding will be UTF-16, and we will convert to and from UTF-8 for tree-sitter purposes. This makes it 
  so all cases are consistent.
  
    
### NOTES TO SELF
1. Zero-width characters - characters that exist but don't advance the position
- will need to keep track of workspace folders 
- good ref: https://github.com/huggingface/llm-ls/blob/main/crates/llm-ls/src/main.rs


### FEATURES TO IMPLEMENT
1. textDocument/completion - provide completions as one types (show built in functions, constants, etc)
   2. We could make this context aware with trigger characters (like $ or @ or :)
2. textDocument/definition - go to a class/ method based on an identifier
3. textDocument/documentHighlight (use tree-sitter highlights for this)
4. textDocument/formatting (need to figure out what formatting is wanted, prob triggered by newline)
5. hover - display documentation for symbols under the cursor
6. signature help - shows function signatures and parameter info with triggers ( , and =
7. Go to def (navigate to symbol defs)
8. Go to impl (navigate to impl)
9. Find references - locates all references to a symbol 
10. Document symbols - list functions and variables in the current doc
11. Workspace symbols - searches for symbols across the workspace


### DATA STRUCTURES 

#### RwLock: Arc<RwLock<HashMap<Url, Arc<ProjectState>>>>
- To get mutable access of the HashMap, we have to lock the RwLock: `.write()` locks the RwLock. From there, we can do `.insert()`
- Without arc, there is single ownership, and therefore this can't be shared across threads. With arc, you have shared 
- ownership across threads, which is exactly what we need to do the indexing.

#### BackendWrapper
This was needed so I could spawn a new async task that runs concurrently on the tokio runtime. Since we are not awaiting the results, this task
runs in the background. 

In the initialized method, we are spawning separate tasks for each workspace folder so that multiple workspaces can be indexed concurrently, and 
the initialized method can return immediately without waiting for indexing to complete. Each task has it's own Arc<Backend> clone, allowing safe 
concurrent access to shared state. 

Cloning the Arc<Backend> creates a new pointer to the same Backend instance, which allows all tasks to see the same updates.


### Potential Improvements: 
1. Debating using Rope or String. String is easier, because we can just do as_bytes.
2. Replace panics with  anyhow

*** THINGS TO DO 
1. Need to define the tree structures in code. This will be necessary to have good code completion. 
2. I think I want to separate public variable hashmaps based on type (Ex: class instance, literal, extrinsic functions, etc).


DECIDING BETWEEN TWO DIFFERENT ARCHITECTURES: 
1. Almost like an intermediate representation, we could code the nodes in rust and this would make code completion really easy. 
   Then also separate the global hashmaps based on type (ie: class method call, literal, etc). This would make semantic checking 
   much easier.
2. Don't code the nodes in rust, this is significantly easier to implement. But semantic checks won't actually happen.
   This way seems to be the way the other language server went. As it allows me to do something like concatenating an 
   instance method call that creates a new class instance with a literal. 
