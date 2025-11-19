mod config;
mod document;
mod lsp;
mod scope_tree;
mod server;
mod test;
mod workspace;

use server::BackendWrapper;
use std::sync::Arc;
use tower_lsp::{LspService, Server};

/*
Arc<RwLock<...>>: Provides thread-safe shared access to the document storage, since LSP methods are async and may be called concurrently
HashMap<Url, DocumentState>: Maps document URIs to their state (text content and parsed AST)
 */

// perhaps I want to store two different tree maps (for .cls files and for .mac/.int files)

// #[derive(Default)]
// struct State {
//     docs: RwLock<HashMap<Url, Doc>>,
// }

///
/// Incremental Parsing: keep each doc's last Tree
/// parser.parse(oldTree). This gives super fast updates as user types

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Arc::new(BackendWrapper::new(client)));
    Server::new(stdin, stdout, socket).serve(service).await;
}
