use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Language, Parser};
use objectscript_udl;

#[derive(Debug)]
struct Backend {
    client: Client, // stored in the backend, and used to send messages/diagnostics to the editor
    // I think we want to include a HashMap that stores the tree values?
    // state: Arc<State>,
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
fn parse(source: &str) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&objectscript_udl::language())
        .unwrap();
    parser.parse(source, None) // returns AST
}

impl Backend {
    fn parse_and_publish() {
        // look into client.publish_diagnostics
        // STEPS :
        // 1. create the AST (or use existing one?)
        // 2.
    }
}

fn main() {}