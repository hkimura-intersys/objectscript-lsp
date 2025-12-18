

use crate::server::BackendWrapper;
use std::sync::Arc;
use tower_lsp::{LspService, Server};
 mod config;
 mod document;
 mod lsp;
 mod scope_tree;
 mod server;
 mod test;
 mod workspace;
 mod parse_structures;
 mod semantic;

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
