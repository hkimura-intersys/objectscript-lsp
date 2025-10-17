mod ls_client_structures;
use tower_lsp::jsonrpc::Result;
use crate::ls_client_structures::*;
use std::pin::Pin;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Language, Parser, Tree};
use objectscript_udl;

struct Backend {
    client: Client, // stored in the backend, and used to send messages/diagnostics to the editor
    // I think we want to include a HashMap that stores the tree values?
    // state: Arc<State>,
    parser: Parser
}

// #[derive(Default)]
// struct State {
//     docs: RwLock<HashMap<Url, Doc>>,
// }

///GO TO DEF:
/// NODE TYPES:
/// class def
/// method def
/// class method calls (references to navigate to)
/// Class Names

/// Parses given text
///
/// Use the tree sitter objectscript to parse the text into a tree
///
/// Incremental Parsing: keep each doc's last Tree and call tree.edit(inputEdit) befpre
/// parser.parse(oldTree). This gives super fast updates as user types

impl LanguageServer for Backend {
    async fn initialize(& self, params: InitializeParams) -> Result<InitializeResult>
    {
        // Check what encodings the client supports
        let client_encodings = params.capabilities
            .general
            .and_then(|g| g.position_encodings);

        // Choose UTF-8 if supported, otherwise fall back to UTF-16
        let encoding = if client_encodings
            .as_ref()
            .map(|e| e.contains(&PositionEncodingKind::UTF8))
            .unwrap_or(false)
        {
            PositionEncodingKind::UTF8
        } else {
            PositionEncodingKind::UTF16  // Default fallback
        };

        // NOTE:

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(encoding),
                text_document_sync:Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(false),
                    will_save_wait_until: Some(false),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    }))
                })),
                ..Default::default()

            },
            ..Default::default() // see if should replace, this represents server_info
        })

    }

    async fn initialized(&self, params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self)
    {
        todo!()
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        todo!()
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        todo!()
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        todo!()
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        todo!()
    }

}

impl Backend {
    pub fn new(client: Client) -> Self {
        // look into client.publish_diagnostics
        let mut parser = Parser::new();
        parser.set_language(&objectscript_udl::language()).unwrap();

        Self {
            client,
            parser
        }
        // STEPS :
    }

    pub fn parse(&mut self, source: &str) -> Option<Tree> {
        self.parser.parse(source, None) // returns AST
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}