use crate::server::BackendWrapper;
use std::sync::Arc;
use tower_lsp::{LspService, Server};
mod class;
mod common;
mod config;
mod document;
mod global_semantic;
mod local_semantic;
mod lsp;
mod method;
mod override_index;
mod parse_structures;
mod scope_structures;
mod scope_tree;
mod server;
mod test;
mod variable;
mod workspace;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Arc::new(BackendWrapper::new(client)));
    Server::new(stdin, stdout, socket).serve(service).await;
}
