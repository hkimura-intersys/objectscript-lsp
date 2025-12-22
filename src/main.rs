use crate::server::BackendWrapper;
use std::sync::Arc;
use tower_lsp::{LspService, Server};
mod config;
mod document;
mod lsp;
mod parse_structures;
mod scope_tree;
mod local_semantic;
mod server;
mod test;
mod workspace;
mod scope_structures;
mod global_semantic;
mod class;
mod common;
mod method;

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
