use crate::scope_tree::ScopeTree;
use crate::workspace::{cls_is_scope_node, walk_tree};
use tower_lsp::lsp_types::Url;
use tree_sitter::ffi::ts_node_descendant_for_point_range;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};
use tree_sitter_objectscript::*;
// TODO: what is the difference between anyhow and panic

// this func will return node, and then whatever is calling this should convert to utf8 text
// if let some(identifier) =



fn test_walking_tree() {
    let code = r#"
Class Demo.Test
{
  ClassMethod Main() [ Private ]
  {
    FOR num=val WRITE num*3 QUIT
  }
  }

  Method test()
  {
  set y = "hi"
  }

}
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&LANGUAGE_OBJECTSCRIPT.into())
        .expect("Error loading Objectscript grammar");
    let tree = parser.parse(code, None).unwrap();
    let mut scope_tree = ScopeTree::new(code.to_string());
    let mut scope_stack = vec![scope_tree.root];
    walk_tree(tree.root_node(), &mut |node| {
        if cls_is_scope_node(node) {
            let scope_id = scope_tree.add_scope(
                node.start_position(),
                node.end_position(),
                *scope_stack.last().unwrap(),
                None,
                false,
            );
            scope_stack.push(scope_id);
            match node.kind() {
                "core_method_body_content" => {
                    // let descendant = node.descendant_for_point_range(node.start_position(),
                    //                                                  node.end_position());
                    // println!("descendant: {:?}", descendant);
                    // println!("node name {:?}", node.grammar_name());
                    // println!("Child count {:?}", node.child_count());
                    let mut cursor = node.walk();
                    let children:Vec<_> = node.children(&mut cursor).collect();
                    println!("Method Children: {:?}", children);
                    for child in children {
                        let children:Vec<_> = child.children(&mut cursor).collect();
                        println!("Statement Children: {:?}", children);
                        for child in children {
                            let children:Vec<_> = child.children(&mut cursor).collect();
                            println!("For loop Children: {:?}", children);
                        }
                    }
                    // let child_by_field_name: Vec<_> = node.children_by_field_name("command_name", &mut cursor).collect();
                    // println!("Child Name: {:?}", child_by_field_name);
                    // get_smallest_child_def(node,true);
                }
                _ => {
                    println!("hi")
                }
            }
        }
    });
}
fn test_query() {
    let code = r#"
Class Demo.Test
{
  ClassMethod Main() [ Private ]
  {
    set x = 42
    set t = true
    FOR num=val WRITE num*3 QUIT
  }

  Method test()
  {
  set y = "hi"
  }

}
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&LANGUAGE_OBJECTSCRIPT.into())
        .expect("Error loading Objectscript grammar");
    let tree = parser.parse(code, None).unwrap();
    let query_str = "(method_definition
(identifier) @method_name
(arguments) ?
(return_type) ?
(method_keywords
(kw_Private) ? @private_method
) ?
(core_method_body_content
(statement
(command_set
(keyword_set) (set_argument
(glvn
(lvn
(objectscript_identifier) @identifier)))))) @method_body
)";
    let query = Query::new(&LANGUAGE_OBJECTSCRIPT.into(), query_str).unwrap();
    let mut query_cursor = QueryCursor::new();
    let mut query_matches = query_cursor.matches(&query, tree.root_node(), code.as_bytes());
    while let Some(query_match) = query_matches.next() {
        // should only be one match (one class per cls file)

        for capture in query_match.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            println!("start: {:?}", Some(capture.node.start_position()));
            println!("range: {:?}", capture.node.range());
            println!("capture_name: {:?}", capture_name);
            println!("NODE TYPE: {:?}", capture.node.kind());
            println!("{:?}", capture.node.utf8_text(code.as_bytes()).unwrap());
        }
        println!("LOOP");
    }
}

// look into using canonicalize
mod tests {
    #[test]
    fn it_works() {
        super::test_walking_tree();
    }
}

/////////////////////
// MISSING PIECES:
/*
BUILDING SCOPE TREE FROM TREE SITTER :
impl ProjectState {
    fn build_scope_tree(&self, url: &Url) -> ScopeTree {
        let tree = {
            let documents = self.documents.read();
            let document = documents.get(url).unwrap();
            document.tree.clone().expect("Failed to get tree from document")
        };

        let mut scope_tree = ScopeTree::new();
        let mut scope_stack = vec![scope_tree.root];

        // Walk the tree-sitter tree
        walk_tree(tree.root_node(), &mut |node| {
            if is_scope_node(node) {
                let scope_id = scope_tree.add_scope(
                    node.start_position(),
                    node.end_position(),
                    *scope_stack.last().unwrap()
                );
                scope_stack.push(scope_id);
            }

            // Extract symbols from this node
            if let Some(symbol) = extract_symbol(node) {
                let current_scope_id = *scope_stack.last().unwrap();
                let mut scopes = scope_tree.scopes.write();
                if let Some(scope) = scopes.get_mut(&current_scope_id) {
                    scope.symbols.insert(symbol.name.clone(), symbol);
                }
            }
        });

        scope_tree
    }

    HELPER FUNCTION FOR TREE WALKING
    // Recursively walk the tree-sitter tree
fn walk_tree<F>(node: tree_sitter::Node, callback: &mut F)
where
    F: FnMut(tree_sitter::Node),
{
    callback(node);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(child, callback);
    }
}

// Determine if a node creates a new scope
fn is_scope_node(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "method_definition" | "class_definition" | "block_statement"
        // Add other ObjectScript scope-creating node types
    )
}

// Extract symbol information from a node
fn extract_symbol(node: tree_sitter::Node) -> Option<Symbol> {
    match node.kind() {
        "variable_declaration" => {
            // Extract variable name and create Symbol
            // This depends on your ObjectScript grammar
            Some(Symbol {
                name: get_identifier_from_node(node),
                range: node.range(),
                kind: SymbolKind::Variable,
                is_global: false,
            })
        }
        "method_definition" => {
            Some(Symbol {
                name: get_identifier_from_node(node),
                range: node.range(),
                kind: SymbolKind::Method,
                is_global: false,
            })
        }
        // Add other symbol types
        _ => None,
    }
}

fn get_identifier_from_node(node: tree_sitter::Node) -> String {
    // Extract the identifier name from the node
    // This depends on your ObjectScript grammar
    // You'll need to find the child node that contains the name
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source_code).ok())
        .unwrap_or("")
        .to_string()

IMPLEMENTING GO TO DEFINITION
#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Convert LSP Position to tree-sitter Point
        let point = Point::new(position.line as usize, position.character as usize);

        // Get the scope tree for this file
        let scope_tree = {
            let local_defs = self.project_state.local_defs.read();
            local_defs.get(&uri)?.clone()
        };

        // Find the scope at the cursor position
        let scope_id = scope_tree.find_current_scope(point)?;

        // Get the identifier at the cursor position
        let identifier = self.get_identifier_at_position(&uri, point)?;

        // Find the declaration
        if let Some(symbol) = scope_tree.find_declaration(&identifier, scope_id) {
            // Convert tree-sitter Range to LSP Range
            let range = Range::new(
                Position::new(symbol.range.start_point.row as u32, symbol.range.start_point.column as u32),
                Position::new(symbol.range.end_point.row as u32, symbol.range.end_point.column as u32),
            );

            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range,
            })));
        }

        // Check global definitions
        let global_defs = self.project_state.global_defs.read();
        if let Some(locations) = global_defs.get(&identifier) {
            if let Some((def_uri, def_point)) = locations.first() {
                let range = Range::new(
                    Position::new(def_point.row as u32, def_point.column as u32),
                    Position::new(def_point.row as u32, def_point.column as u32),
                );

                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: def_uri.clone(),
                    range,
                })));
            }
        }

        Ok(None)
    }
}

GET IDENTIFIER AT POSITION (Extract the identifier at the cursor)
impl Backend {
    fn get_identifier_at_position(&self, uri: &Url, point: Point) -> Option<String> {
        let documents = self.project_state.documents.read();
        let document = documents.get(uri)?;
        let tree = document.tree.as_ref()?;

        // Find the node at the cursor position
        let node = tree.root_node().descendant_for_point_range(point, point)?;

        // Get the identifier text
        if node.kind() == "identifier" {
            let source = document.content.to_string();
            return node.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
        }

        None
    }
}

UPDATING SCOPE TREES ON DOCUMENT CHANGES :
#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;

        // Parse and create document
        // ... (your existing code)

        // Build scope tree
        let scope_tree = self.project_state.build_scope_tree(&uri);
        self.project_state.local_defs.write().insert(uri.clone(), scope_tree);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Update document
        // ... (your existing code)

        // Rebuild scope tree
        let scope_tree = self.project_state.build_scope_tree(&uri);
        self.project_state.local_defs.write().insert(uri.clone(), scope_tree);
    }
}
 */
