// // src/main.rs
// use anyhow::Result;
// use ropey::Rope;
// use std::{collections::HashMap, sync::Arc};
// use tokio::sync::RwLock;
// use tower_lsp::lsp_types::*;
// use tower_lsp::{Client, LanguageServer, LspService, Server};
//
// mod lang; // objectscript_udl() -> Language
//
// #[derive(Default)]
// struct Doc {
//     text: Rope,
// }
//
// #[derive(Default)]
// struct State {
//     docs: RwLock<HashMap<Url, Doc>>,
// }
//
// #[derive(Clone)]
// struct Backend {
//     client: Client,
//     state: Arc<State>,
// }
//
// #[tower_lsp::async_trait]
// impl LanguageServer for Backend {
//     async fn initialize(
//         &self,
//         _: InitializeParams,
//     ) -> tower_lsp::jsonrpc::Result<InitializeResult> {
//         Ok(InitializeResult {
//             capabilities: ServerCapabilities {
//                 text_document_sync: Some(TextDocumentSyncCapability::Kind(
//                     TextDocumentSyncKind::INCREMENTAL,
//                 )),
//                 // add more capabilities later (hover, semantic tokens, completion, etc.)
//                 ..Default::default()
//             },
//             ..Default::default()
//         })
//     }
//
//     async fn initialized(&self, _: InitializedParams) {
//         let _ = self
//             .client
//             .log_message(MessageType::INFO, "ObjectScript LSP initialized");
//     }
//
//     async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
//         Ok(())
//     }
//
//     async fn did_open(&self, params: DidOpenTextDocumentParams) {
//         let uri = params.text_document.uri;
//         let text = Rope::from_str(&params.text_document.text);
//         {
//             let mut docs = self.state.docs.write().await;
//             docs.insert(uri.clone(), Doc { text });
//         }
//         self.reparse_and_publish(&uri, Some(params.text_document.version))
//             .await;
//     }
//
//     async fn did_change(&self, params: DidChangeTextDocumentParams) {
//         let uri = params.text_document.uri;
//         let version = params.text_document.version;
//         if let Some(doc) = self.state.docs.write().await.get_mut(&uri) {
//             for change in params.content_changes {
//                 match change.range {
//                     None => {
//                         // Full sync
//                         doc.text = Rope::from_str(&change.text);
//                     }
//                     Some(range) => {
//                         // Apply incremental edit
//                         let (start, end) = lsp_range_to_char_span(&doc.text, range);
//                         doc.text.remove(start..end);
//                         doc.text.insert(start, &change.text);
//                     }
//                 }
//             }
//         }
//         self.reparse_and_publish(&uri, Some(version)).await;
//     }
//
//     async fn did_close(&self, params: DidCloseTextDocumentParams) {
//         let uri = params.text_document.uri;
//         self.state.docs.write().await.remove(&uri);
//         // Clear diagnostics
//         let _ = self.client.publish_diagnostics(uri, vec![], None).await;
//     }
// }
//
// impl Backend {
//     async fn reparse_and_publish(&self, uri: &Url, version: Option<i32>) {
//         let Some(doc) = self.state.docs.read().await.get(uri) else {
//             return;
//         };
//         let mut parser = tree_sitter::Parser::new();
//         parser
//             .set_language(lang::objectscript_udl())
//             .expect("load ObjectScript UDL language");
//         // Parse (full reparse for simplicity; can be made incremental)
//         let tree = parser.parse(doc.text.to_string(), None);
//
//         let mut diags = Vec::new();
//         if let Some(tree) = tree {
//             let mut cursor = tree.walk();
//             let root = tree.root_node();
//             // Collect ERROR nodes as basic syntax diagnostics
//             for node in root.descendants(&mut cursor) {
//                 if node.is_error() {
//                     let range = ts_range_to_lsp(&doc.text, node.range());
//                     diags.push(Diagnostic {
//                         range,
//                         severity: Some(DiagnosticSeverity::ERROR),
//                         message: "Syntax error".into(),
//                         ..Default::default()
//                     });
//                 }
//             }
//         }
//
//         let _ = self
//             .client
//             .publish_diagnostics(uri.clone(), diags, version)
//             .await;
//     }
// }
//
// // --- UTF-16 position helpers (LSP uses UTF-16 code units) ---
// fn lsp_range_to_char_span(text: &Rope, range: Range) -> (usize, usize) {
//     (
//         lsp_pos_to_char_idx(text, range.start),
//         lsp_pos_to_char_idx(text, range.end),
//     )
// }
//
// fn lsp_pos_to_char_idx(text: &Rope, pos: Position) -> usize {
//     // convert (line, utf16_col) -> char index
//     let line = pos.line as usize;
//     let utf16_col = pos.character as usize;
//     let line_start_char = text.line_to_char(line);
//     let line_text = text.line(line).to_string();
//     // walk the line until we reach the Nth UTF-16 code unit
//     let mut units = 0usize;
//     let mut chars = 0usize;
//     for ch in line_text.chars() {
//         let add = ch.encode_utf16(&mut [0; 2]).len();
//         if units >= utf16_col {
//             break;
//         }
//         units += add;
//         chars += 1;
//     }
//     line_start_char + chars
// }
//
// fn ts_range_to_lsp(text: &Rope, r: tree_sitter::Range) -> Range {
//     let start_line = r.start_point.row as u32;
//     let end_line = r.end_point.row as u32;
//
//     let start_col_utf16 =
//         utf8_col_to_utf16_col(text, r.start_point.row as usize, r.start_point.column);
//     let end_col_utf16 = utf8_col_to_utf16_col(text, r.end_point.row as usize, r.end_point.column);
//
//     Range {
//         start: Position {
//             line: start_line,
//             character: start_col_utf16,
//         },
//         end: Position {
//             line: end_line,
//             character: end_col_utf16,
//         },
//     }
// }
//
// fn utf8_col_to_utf16_col(text: &Rope, line: usize, utf8_col: usize) -> u32 {
//     let s = text.line(line).to_string();
//     let up_to = &s[..utf8_col.min(s.len())];
//     up_to.encode_utf16().count() as u32
// }
//
// #[tokio::main]
// async fn main() -> Result<()> {
//     let stdin = tokio::io::stdin();
//     let stdout = tokio::io::stdout();
//     let (service, socket) = LspService::new(|client| Backend {
//         client,
//         state: Arc::new(State::default()),
//     });
//     Server::new(stdin, stdout, socket).serve(service).await;
//     Ok(())
// }
