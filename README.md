# OBJECTSCRIPT LANGUAGE SERVER USING TOWER_LS

## Overview
This implementation uses the tower_ls rust trait to implement an objectscript language server. 

The languageServer trait defines the capabilities of our languageServer. The client represents any LSP compliant IDE.
Note that all things related to the client are sent through the IDE.

In the `initialization()` function, we define the [server capabilities](https://docs.rs/lsp-types/latest/lsp_types/struct.ServerCapabilities.html).
The 


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
  - Due to this, we need to be able to convert between UTF-8 and UTF-16


### NOTES TO SELF
1. Zero-width characters - characters that exist but don't advance the position

