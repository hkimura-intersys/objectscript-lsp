use crate::parse_structures::{ClassId, CodeMode, Language, MethodCallSite, ReturnType, UnresolvedCallSite, VarType};
use crate::scope_structures::{ClassGlobalSymbolId, ScopeId};
use crate::scope_tree::ScopeTree;
use serde_json::Value;
use std::ops::Range as CoreRange;
use tower_lsp::lsp_types::{Position, Range as LspRange, Url};
use tree_sitter::{Node, Point, Range as TsRange, Range, Tree};
use std::collections::HashMap;
use crate::override_index::OverrideIndex;

pub fn print_statements_exit_method_overrides_fn(
    method_name: &str,
    superclass_name: &str,
    locations: Vec<(Url, Range)>,
) {
    if locations.is_empty() {
        eprintln!("Leaving ProjectData function: get_variable_symbol_location.., there are no overrides of method {:?} from superclass {:?}", method_name, superclass_name);
        eprintln!("------------------------");
        eprintln!();
        return;
    }
    eprintln!("Leaving ProjectData function: get_variable_symbol_location.. the locations for method overrides of the method named: {:?} in the superclass {:?} are:  \n {:?}", method_name, superclass_name, locations);
    eprintln!("------------------------");
    eprintln!();
}

pub fn point_to_lsp_position(text: &str, p: Point) -> Position {
    start_of_function("COMMON (no struct)", "point_to_lsp_position");
    let starts = line_starts(text);
    let (line_start, _line_end_incl, line_end_excl) = line_bounds(text, &starts, p.row);

    // If point is on EOF row, map to UTF-16 character 0
    if line_start == text.len() && line_end_excl == text.len() {
        eprintln!("Info: Point is on EOF row, mapping to UTF-16 character 0");
        return Position {
            line: p.row as u32,
            character: 0,
        };
    }

    // Clamp column to visible line (exclude '\n')
    let max_col = line_end_excl.saturating_sub(line_start);
    let target_col = p.column.min(max_col);

    let line = &text[line_start..line_end_excl];

    let mut bytes = 0usize;
    let mut utf16_units = 0u32;

    for ch in line.chars() {
        let ch_bytes = ch.len_utf8();
        if bytes + ch_bytes > target_col {
            break;
        }
        bytes += ch_bytes;
        utf16_units += ch.len_utf16() as u32;
        if bytes == target_col {
            break;
        }
    }

    eprintln!("Position is: line: {}, character: {}", p.row as u32, utf16_units);
    successful_exit("COMMON (no struct)", "point_to_lsp_position");

    Position {
        line: p.row as u32,
        character: utf16_units,
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    start_of_function("COMMON (no struct)", "line_starts");
    let mut starts = Vec::new();
    starts.push(0); // line 0 starts at byte 0

    for (i, b) in text.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            starts.push(i + 1); // next line starts right after '\n'
        }
    }

    starts.push(text.len()); // sentinel
    successful_exit("COMMON (no struct)", "line_starts");
    starts
}

/// Returns byte bounds for a specific line in `text` using a precomputed line-start table.
///
/// `starts` is a slice of byte offsets where each element is the start index of a line.
/// It must include a final **sentinel** entry equal to `text.len()` (or the byte offset
/// immediately after the last line), so `starts.len() == number_of_lines + 1`.
///
/// For a valid `row` (0-based), this function returns:
/// - `start`: the byte offset where line `row` begins.
/// - `end_incl`: the byte offset of the start of the *next* line (i.e. one past the end of this line,
///   including the trailing `\n` if present).
/// - `end_excl`: the byte offset one past the end of the line content, excluding a trailing `\n` if present.
///
/// If `row` is out of range (`row >= starts.len() - 1`), returns `(len, len, len)` where `len = text.len()`.
///
/// - If `starts` is missing the sentinel, too short, or contains out-of-range offsets,
///   prints a warning and returns `(len, len, len)`.
fn line_bounds(text: &str, starts: &[usize], row: usize) -> (usize, usize, usize) {
    start_of_function("COMMON (no struct)", "line_bounds");

    let len = text.len();

    // Need at least one line start + sentinel
    if starts.len() < 2 {
        eprintln!(
            "Warning: line_bounds: invalid starts table (len={}), expected at least 2 (including sentinel).",
            starts.len()
        );
        generic_exit_statements("COMMON (no struct)", "line_bounds");
        return (len, len, len);
    }

    // Sentinel should typically be == text.len()
    let sentinel = *starts.last().unwrap();
    if sentinel > len {
        eprintln!(
            "Warning: line_bounds: sentinel {} out of bounds for text len {}.",
            sentinel, len
        );
        generic_exit_statements("COMMON (no struct)", "line_bounds");
        return (len, len, len);
    }

    let eof_row = starts.len() - 1; // last entry is sentinel
    if row >= eof_row {
        // out of range row => "EOF bounds"
        successful_exit("COMMON (no struct)", "line_bounds");
        return (len, len, len);
    }

    let start = starts[row];
    let end_incl = starts[row + 1];

    // Validate monotonic + in-bounds
    if start > end_incl || end_incl > len {
        eprintln!(
            "Warning: line_bounds: invalid bounds for row {}: start={}, end_incl={}, text_len={}.",
            row, start, end_incl, len
        );
        generic_exit_statements("COMMON (no struct)", "line_bounds");
        return (len, len, len);
    }

    let end_excl = if end_incl > start && text.as_bytes().get(end_incl - 1) == Some(&b'\n') {
        end_incl - 1
    } else {
        end_incl
    };

    successful_exit("COMMON (no struct)", "line_bounds");
    (start, end_incl, end_excl)
}

/// Converts an LSP `Position` (line + UTF-16 character offset) into a Tree-sitter `Point`
/// (row + UTF-8 byte column) for the given source `text`.
///
/// LSP positions encode `character` as a count of UTF-16 code units from the start of the line.
/// Tree-sitter points encode `column` as a byte offset (UTF-8) from the start of the line.
/// This function bridges those two coordinate systems by:
/// 1) locating the requested line bounds in `text`, and
/// 2) walking the line’s Unicode scalar values to convert a UTF-16 offset into a UTF-8 byte column.
///
/// If `position.line` is at or beyond EOF (as determined by `line_bounds`), this returns an
/// EOF-like point with `{ row, column: 0 }`.
///
/// If `position.character` lands in the middle of a surrogate pair boundary (i.e. between the
/// two UTF-16 code units used by a single non-BMP character), the conversion stops early and
/// returns the column at the start of that character (it does not split the pair).
///
/// # Notes
/// - `row` is zero-based, matching both LSP and Tree-sitter.
/// - The returned `column` is a byte offset within the line (0-based).
pub fn position_to_point(text: &str, position: Position) -> Point {
    start_of_function("COMMON (no struct)", "position_to_point");
    let starts = line_starts(text);
    let row = position.line as usize;

    let (line_start, _line_end_incl, line_end_excl) = line_bounds(text, &starts, row);

    // If row is EOF (or beyond), return EOF point
    if line_start == text.len() && line_end_excl == text.len() {
        return Point { row, column: 0 };
    }

    let line = &text[line_start..line_end_excl];

    // Convert UTF-16 units to byte offset within this line
    let mut remaining = position.character as usize;
    let mut col_bytes = 0usize;

    for ch in line.chars() {
        if remaining == 0 {
            break;
        }
        let u16 = ch.len_utf16();
        if remaining < u16 {
            break; // don't split a surrogate pair
        }
        remaining -= u16;
        col_bytes += ch.len_utf8();
    }
    successful_exit("COMMON (no struct)", "position_to_point");
    Point {
        row,
        column: col_bytes,
    }
}

/// Converts a Tree-sitter `Range` into an LSP `Range` for the given source `text`.
///
/// Tree-sitter ranges are expressed as start/end `Point`s where the `column` is a UTF-8 byte
/// offset within the line. LSP ranges are expressed as start/end `Position`s where the
/// `character` is a UTF-16 code-unit offset within the line.
///
/// This function performs the conversion by translating both `start_point` and `end_point`
/// via `point_to_lsp_position`.
pub fn ts_range_to_lsp_range(text: &str, r: TsRange) -> LspRange {
    start_of_function("COMMON (no struct)", "ts_range_to_lsp_range");
    let start = point_to_lsp_position(text, r.start_point);
    let end = point_to_lsp_position(text, r.end_point);
    successful_exit("COMMON (no struct)", "ts_range_to_lsp_range");
    LspRange {
        start,
        end,
    }
}

/// Converts a Tree-sitter `Point` (row + UTF-8 byte column) into an absolute byte offset
/// into `text`.
///
/// This function uses a precomputed line-start table (`line_starts`) and `line_bounds` to:
/// - find the start of `point.row`,
/// - interpret `point.column` as a UTF-8 byte offset within that line, and
/// - return the absolute byte index `line_start + column`.
///
/// Behavior at boundaries:
/// - If `point.row` is at or beyond the EOF row (based on the sentinel in `line_starts`),
///   this returns `text.len()`.
/// - The column is clamped to the end of the line content (excluding a trailing `'\n'`),
///   so the returned offset will not point past the line’s non-newline characters.
///
/// # Notes
/// - `point.row` and `point.column` are both zero-based.
/// - The returned value is a byte index into `text` (suitable for slicing on UTF-8
///   boundaries, assuming `point.column` came from Tree-sitter / valid byte columns).
pub fn point_to_byte(text: &str, point: Point) -> usize {
    start_of_function("COMMON (no struct)", "point_to_byte");
    let starts = line_starts(text);

    // starts has a sentinel at text.len(), so EOF row is starts.len() - 1
    let eof_row = starts.len().saturating_sub(1);

    // If point is on EOF row (or beyond), it's EOF byte offset
    if point.row >= eof_row {
        eprintln!("Info: reached the EOF row");
        successful_exit("COMMON (no struct)", "point_to_byte");
        return text.len();
    }

    let (line_start, _line_end_incl, line_end_excl) = line_bounds(text, &starts, point.row);

    // Clamp column to the visible line (excluding '\n')
    let max_col = line_end_excl.saturating_sub(line_start);
    let col = point.column.min(max_col);
    successful_exit("COMMON (no struct)", "point_to_byte");
    line_start + col
}

/// Advances `(row, column)` by `changed_text`, returning the resulting `Point`.
///
/// Newlines increment `row` and reset `column`; other chars add their UTF-8 byte length.

pub fn advance_point(mut row: usize, mut column: usize, changed_text: &str) -> Point {
    start_of_function("COMMON (no struct)", "advance_point");
    for c in changed_text.chars() {
        if c == '\n' {
            row += 1;
            column = 0;
        } else {
            column += c.len_utf8();
        }
    }
    successful_exit("COMMON (no struct)", "advance_point");
    Point { row, column }
}

/// Returns a Vec of all named children nodes for a given Tree Sitter Node.
pub fn get_node_children(node: Node) -> Vec<Node> {
    start_of_function("COMMON (no struct)", "get_node_children");
    let mut cursor = node.walk();
    let result = node.named_children(&mut cursor).collect::<Vec<Node>>();
    successful_exit("COMMON (no struct)", "get_node_children");
    result
}

/// Given a Node, finds if there is a class definition child node. If so, returns that.
pub fn find_class_definition(root: Node) -> Option<Node> {
    start_of_function("COMMON (no struct)", "find_class_definition");
    let mut cursor = root.walk();
    let result = root
        .named_children(&mut cursor)
        .find(|n| n.kind() == "class_definition");
    match result {
        None => {
            eprintln!(
                "Error: Could not find class definition node from tree: {:?} \n\n\n",
                root.to_sexp()
            );
            generic_exit_statements("COMMON (no struct)", "class_definition");
            result
        }
        Some(_) => {
            successful_exit("COMMON (no struct)", "class_definition");
            result },
    }
}

/// Extracts the class name from a parsed Tree-sitter root `node`.
///
/// Finds the `class_definition` node (via `find_class_definition`), then reads the class name
/// from its second named child (index `1`) and slices it from `content` using the node’s byte range.
///
/// Returns `None` if no class definition/name is found or if the byte range is invalid; prints a
/// warning on unexpected/mismatched structure.
pub fn get_class_name_from_root(content: &str, node: Node) -> Option<String> {
    start_of_function("COMMON (no struct)", "get_class_name_from_root");
    let Some(class_def) = find_class_definition(node) else {
        return None;
    };

    // class_definition children are:
    // 0: keyword_class, 1: class_name identifier, 2: class_body ...
    let Some(name_node) = class_def.named_child(1) else {
        eprintln!("Warning: Expected Class name node to be at the class definition ({:?}) node's 1 index, but it was not", class_def);
        generic_exit_statements("COMMON (no struct)", "get_class_name_from_root");
        return None;
    };

    let Some(class_name) = content.get(name_node.byte_range()) else {
        eprintln!("Warning: Failed to get class name from content: {:?} \n\n\n. Expected it to be at byte range {:?}", content, name_node);
        generic_exit_statements("COMMON (no struct)", "get_class_name_from_root");
        return None;
    };
    successful_exit("COMMON (no struct)", "get_class_name_from_root");
    Some(class_name.to_string())
}

/// Returns the substring for `range` (byte offsets) within `content`.
///
/// Logs a warning and returns `None` if the range is out of bounds.
pub fn get_string_at_byte_range(content: &str, range: CoreRange<usize>) -> Option<String> {
    start_of_function("COMMON (no struct)", "get_string_at_byte_range");
    let Some(s) = content.get(range) else {
        eprintln!("Couldn't get string from given byte range");
        generic_exit_statements("COMMON (no struct)", "get_string_at_byte_range");
        return None;
    };
    successful_exit("COMMON (no struct)", "get_string_at_byte_range");
    Some(s.to_string())
}

/// Infers a `VarType` from the first child of an `expr_atom`-like node.
///
/// Matches common literal/identifier forms and extracts names from `content` using the node’s
/// byte range when needed (e.g., `gvn`, `lvn`, instance variables). Returns `None` for unknown
/// or unsupported node kinds.
pub fn get_expr_atom_var_type(node: Node, content: &str) -> Option<VarType> {
    start_of_function("COMMON (no struct)", "get_expr_atom_var_type");
    let Some(node) = node.named_child(0) else {
        generic_exit_statements("COMMON (no struct)", "get_expr_atom_var_type");
        return None;
    };
    match node.kind() {
        "json_object_literal" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::JsonObjectLiteral) },
        "json_array_literal" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::JsonArrayLiteral) },
        "string_literal" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::String) },
        "numeric_literal" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::Number) },
        "relative_dot_method" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::RelativeDotMethod) },
        "relative_dot_property" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::RelativeDotProperty) },
        "relative_dot_parameter" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::RelativeDotParameter) },
        "oref_chain_expr" => {
            // either a method call or
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::OrefChainExpr)
        }
        "class_method_call" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::ClassMethodCall) },
        "class_parameter_ref" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::ClassParameterRef) },
        "superclass_method_call" => {
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::SuperclassMethodCall) },
        "gvn" => {
            let var_name = content[node.byte_range()].to_string();
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::Gvn(var_name))
        }
        "lvn" => {
            let var_name = content[node.byte_range()].to_string();
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::Lvn(var_name))
        }
        "instance_variable" => {
            let property_name = match node
                .named_child(0)
                .and_then(|n| content.get(n.byte_range()))
            {
                Some(name) => name.to_string(),
                None => {
                    generic_exit_statements("COMMON (no struct)", "get_expr_atom_var_type");
                    return None },
            };
            successful_exit("COMMON (no struct)", "get_expr_atom_var_type");
            Some(VarType::InstanceVariable(property_name))
        }
        _ => {
            // TODO: macro, ssvn, sql_field_reference
            println!("Unimplemented: {:?}", node.kind());
            generic_exit_statements("get_expr_atom_var_type", "get_expr_atom_var_type");
            None
        }
    }
}

/// Maps a type name string (e.g. InterSystems % types) to a `ReturnType`.
///
/// Unrecognized names return `ReturnType::Other(typename)` and are logged as unimplemented.
pub fn find_return_type(typename: String) -> Option<ReturnType> {
    start_of_function("COMMON (no struct)", "find_return_type");
    let result = match typename.as_str() {
        "%exactstring" | "%enumstring" | "%string" | "%char" => Some(ReturnType::String),
        "%bigint" | "%smallint" | "%integer" | "%posixtime" | "%counter" => {
            Some(ReturnType::Integer)
        }
        "%tinyint" => Some(ReturnType::TinyInteger),
        "%binary" => Some(ReturnType::Binary),
        "%date" => Some(ReturnType::Date),
        "%double" => Some(ReturnType::Double),
        "%numeric" | "%time" => Some(ReturnType::Number),
        "%status" => Some(ReturnType::Status),
        _ => {
            println!("Unimplemented typename: {:?}", typename);
            Some(ReturnType::Other(typename))
        }
    };

    match result {
        Some(_) => {
            successful_exit("COMMON (no struct)", "get_return_type");
            result
        }
        None => {
            generic_exit_statements("get_return_type", "get_return_type");
            result
        }
    }
}

/// Walks an expression node and returns the `VarType`s found within it.
///
/// Handles parenthetical, nested, unary, and simple binary expressions by recursively descending
/// into expression tails. Unsupported operator/atom kinds are skipped with a warning.
pub fn find_var_type_from_expression(node: Node, content: &str) -> Vec<VarType> {
    start_of_function("COMMON (no struct)", "find_var_type_from_expression");
    let mut var_types = Vec::new();
    let children = get_node_children(node);
    let Some(&node_child) = children.get(0) else {
        eprintln!("Error: Failed to get child of node: {:?}", node);
        generic_exit_statements("COMMON (no struct)", "find_var_type_from_expression");
        return Vec::new();
    };
    if node_child.kind() == "_parenthetical_expression" {
        let expression = match children.get(0).and_then(|c| c.named_child(0)) {
            Some(expr) => expr,
            None => {
                generic_exit_statements("COMMON (no struct)", "find_var_type_from_expression");
                return Vec::new() },
        };
        let result = find_var_type_from_expression(expression, content);
        for v in result {
            var_types.push(v);
        }
    }
    else if node_child.kind() == "expression" {
        let result = find_var_type_from_expression(node_child, content);
        for v in result {
            var_types.push(v);
        }
    }
    else if node_child.kind() == "unary_expression" {
        let unary_child = match children.get(0).and_then(|c| c.named_child(0)) {
            Some(expr) => expr,
            None => {
                generic_exit_statements("COMMON (no struct)", "find_var_type_from_expression");
                return Vec::new() },
        };
        if unary_child.kind() == "expression" {
            let result = find_var_type_from_expression(unary_child, content);
            for v in result {
                var_types.push(v);
            }
        }
        else if unary_child.kind() == "glvn" {
            let Some(var_name) = unary_child
                .named_child(0)
                .and_then(|n| content.get(n.byte_range()))
                .map(str::to_string)
            else {
                eprintln!("failed to get var name from unary child node: {:?}", unary_child);
                generic_exit_statements("COMMON (no struct)", "find_var_type_from_expression");
                return var_types;
            };
            var_types.push(VarType::Gvn(var_name));
        }
    }
    else {
        let Some(expr_atom_type) = get_expr_atom_var_type(node_child, content) else {
            eprintln!("Failed to get var type from expr atom node: {:?}", node_child);
            generic_exit_statements("COMMON (no struct)", "find_var_type_from_expression");
            return var_types;
        };
        var_types.push(expr_atom_type);
    }

    for node in children.iter().skip(1) {
        // each node is an expr tail
        let Some(op_node) = node.named_child(0) else {
            eprintln!("Error: Failed to get child at index 0 of node: {:?}", node);
            generic_skipping_statements("find_var_type_from_expression", "Node", "Node");
            continue;
        };

        if op_node.kind() == "binary_operator" {
            let Some(expr_node) = node.named_child(1) else {
                eprintln!("Error: Failed to get child at index 1 of node: {:?}", node);
                generic_skipping_statements("find_var_type_from_expression", "Node", "Node");
                continue;
            };

            let var_types_expr_tail = find_var_type_from_expression(expr_node, content);

            var_types.extend(var_types_expr_tail);
        } else {
            eprintln!("Unimplemented expr atom type: {:?}", node.kind());
            generic_skipping_statements("find_var_type_from_expression", "Node", "Node");
            continue;
        }
    }
    successful_exit("COMMON (no struct)", "find_var_type_from_expression");
    var_types
}

/// Looks up a Tree-sitter keyword child node type by searching the generated node-types JSON.
///
/// `keyword_type` selects the parent node type, and `filter` is matched as a substring against
/// child `"type"` values. Returns the matched type string or `""` if not found/parsable.
pub fn get_keyword(keyword_type: &str, filter: &str) -> String {
    start_of_function("COMMON (no struct)", "get_keyword");
    let json = tree_sitter_objectscript::OBJECTSCRIPT_NODE_TYPES; // &'static str

    let v: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Failed to parse JSON.");
            generic_exit_statements("COMMON (no struct)", "get_keyword");
            return "".to_string() },
    };

    // node-types.json is an array of objects
    let Some(arr) = v.as_array() else {
        eprintln!("Failed to get array of objects from node-types.json");
        generic_exit_statements("COMMON (no struct)", "get_keyword");
        return "".to_string();
    };

    // find the object with "type": keyword_type
    let keyword = arr
        .iter()
        .find(|obj| obj.get("type").and_then(Value::as_str) == Some(keyword_type));

    if let Some(obj) = keyword {
        if let Some(types) = obj
            .get("children")
            .and_then(|c| c.get("types"))
            .and_then(Value::as_array)
        {
            for t in types {
                if let Some(ty) = t.get("type").and_then(Value::as_str) {
                    if ty.contains(filter) {
                        successful_exit("COMMON (no struct)", "get_keyword");
                        return ty.to_string();
                    }
                }
            }
            eprintln!("Failed to find keyword");
            generic_exit_statements("COMMON (no struct)", "get_keyword");
        }
        else {
            eprintln!("Failed to get keyword children from node-types.json");
            generic_exit_statements("COMMON (no struct)", "get_keyword");
        }
    }
    generic_exit_statements("COMMON (no struct)", "get_keyword");
    "".to_string()
}

/// Builds an initial `ScopeTree` skeleton from a parsed `Tree`.
///
/// Creates a new `ScopeTree` rooted at `class_symbol_id`, then walks the syntax tree and adds
/// scopes for nodes considered "scope nodes" (see `cls_is_scope_node`).
pub fn initial_build_scope_tree(tree: Tree, class_symbol_id: ClassGlobalSymbolId) -> ScopeTree {
    start_of_function("COMMON (no struct)", "initial_build_scope_tree");
    let mut scope_tree = ScopeTree::new(class_symbol_id);
    let mut scope_stack = vec![scope_tree.root];

    let root = tree.root_node();
    build_scope_skeleton(root, &mut scope_tree, &mut scope_stack);

    successful_exit("COMMON (no struct)", "initial_build_scope_tree");
    scope_tree
}

/// Recursively traverses `node` and adds scope entries to `scope_tree`, maintaining a stack of
/// active scope ids in `scope_stack`.
fn build_scope_skeleton(node: Node, scope_tree: &mut ScopeTree, scope_stack: &mut Vec<ScopeId>) {
    let is_scope = cls_is_scope_node(node);

    if is_scope {
        let Some(&parent) = scope_stack.last() else {
            eprintln!("Failed to get Scope Parent when building Scope Tree");
            generic_exit_statements("COMMON (no struct)", "build_scope_skeleton");
            return;
        };
        let scope_id =
            scope_tree.add_scope(node.start_position(), node.end_position(), parent, false);
        scope_stack.push(scope_id);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        build_scope_skeleton(child, scope_tree, scope_stack);
    }

    if is_scope {
        scope_stack.pop();
    }
}

/// Returns `true` if `pos` lies in the half-open range `[start, end)`.
/// Returns `false` otherwise.
pub fn point_in_range(pos: Point, start: Point, end: Point) -> bool {
    if pos >= start && pos < end {
        return true;
    };
    false
}


/// Returns `true` if `node` is treated as a scope boundary in `.cls` parsing.
/// Returns `false` otherwise.
pub fn cls_is_scope_node(node: Node) -> bool {
    node.kind() == "classmethod" || node.kind() == "method"
}

/// Walks up the syntax tree from `node` to find the enclosing `method_definition` and returns
/// that method’s name (sliced from `content`).
///
/// Returns `None` if no method definition is found within a bounded parent walk.
pub fn method_name_from_identifier_node(
    node: Node,
    content: &str,
    mut iteration: usize,
) -> Option<String> {
    if iteration == 0 {
        start_of_function("COMMON (no struct)", "method_name_from_identifier_node");
    }
    if iteration > 10 {
        eprintln!("Iteration MAX reached");
        generic_exit_statements("COMMON (no struct)", "method_name_from_identifier_node");
        return None;
    }
    match node.kind() {
        "class_statement" | "class_definition" | "class_body" => {
            eprintln!("Node parent is not method definition");
            successful_exit("COMMON (no struct)", "method_name_from_identifier_node");
            None
        }
        "method_definition" => {
            let Some(method_name_node) = node.named_child(0) else {
                eprintln!("Failed to get method name node from method definition node");
                generic_exit_statements("COMMON (no struct)", "method_name_from_identifier_node");
                return None;
            };
            let Some(method_name) = content.get(method_name_node.byte_range()) else {
                eprintln!("Failed to get method name string from method name node");
                generic_exit_statements("COMMON (no struct)", "method_name_from_identifier_node");
                return None;
            };
            successful_exit("COMMON (no struct)", "method_name_from_identifier_node");
            Some(method_name.to_string())
        }
        _ => {
            iteration += 1;
            let Some(new_node) = node.parent() else {
                eprintln!("Max depth reached, node does not have a parent");
                generic_exit_statements("COMMON (no struct)", "method_name_from_identifier_node");
                return None;
            };
            method_name_from_identifier_node(new_node, content, iteration)
        }
    }
}

/// Scans a method definition and returns all *unresolved* method call sites.
///
/// Currently collects:
/// - `do ##class(Some.Class).Method(args...)` as `class_method_call`
/// - simple relative-dot instance calls (single `relative_dot_method` segment) as calls on
///   `current_class`
///
/// Each result includes the callee class/method name plus the source range of the call node and
/// ranges for each argument expression. Resolution to symbols happens later.
pub fn build_method_calls(
    current_class: &str,
    method_definition_node: Node,
    content: &str,
) -> Vec<UnresolvedCallSite> {
    start_of_function("No struct", "build_method_calls");
    let mut out = Vec::new();
    let children = get_node_children(method_definition_node);
    for child in children.into_iter().skip(1) {
        if child.kind() != "core_method_body_content" {
            continue;
        }

        // each child is a statement
        for statement in get_node_children(child) {
            let Some(cmd) = statement.named_child(0) else {
                eprintln!("Warning: failed to get node child at index 0 for method statement in core method body: {:?}", statement);
                generic_skipping_statements("build_method_calls", "Node", "Node");
                continue;
            };

            match cmd.kind() {
                "command_do" => {
                    let Some(do_arg) = cmd.named_child(1) else {
                        eprintln!("Warning: failed to get node child at index 1 for do command node: {:?}", statement);
                        generic_skipping_statements("build_method_calls", "Node", "Node");
                        continue;
                    };

                    match do_arg.kind() {
                        "class_method_call" => {
                            //  child(0): class_ref
                            //  child(1): method name
                            //  child(2): argument list node
                            let call_range = do_arg.range();
                            let Some(class_ref) = do_arg.named_child(0) else {
                                eprintln!("Warning: failed to get node child at index 0 for do argument node: {:?}", statement);
                                generic_skipping_statements("build_method_calls", "Node", "Node");
                                continue;
                            };
                            let class_ref_name = {
                                let Some(name_node) = class_ref.named_child(1) else {
                                    eprintln!("Warning: failed to get node child at index 1 for class ref node: {:?}", statement);
                                    generic_skipping_statements("build_method_calls", "Node", "Node");
                                    continue;
                                };
                                let Some(s) =
                                    get_string_at_byte_range(content, name_node.byte_range())
                                else {
                                    eprintln!("Warning: failed to get string content from content: {:?} for class name node. Expected content to be a class name.", content);
                                    generic_skipping_statements("build_method_calls", "Node", "Node");
                                    continue;
                                };
                                s
                            };

                            let callee_method = {
                                let Some(m) = do_arg.named_child(1) else {
                                    eprintln!("Warning: failed to get node child at index 1 for do argument node: {:?}", statement);
                                    generic_skipping_statements("build_method_calls", "Node", "Node");
                                    continue;
                                };
                                let Some(s) = get_string_at_byte_range(content, m.byte_range())
                                else {
                                    eprintln!("Warning: failed to get string content from content: {:?} for do argument node. Expected content to be a method name.", content);
                                    generic_skipping_statements("build_method_calls", "Node", "Node");
                                    continue;
                                };
                                s
                            };

                            let arg_ranges: Vec<Range> = do_arg
                                .named_child(2)
                                .map(|args_node| {
                                    get_node_children(args_node)
                                        .into_iter()
                                        .map(|a| a.range())
                                        .collect()
                                })
                                .unwrap_or_else(Vec::new);

                            out.push(UnresolvedCallSite {
                                callee_class: class_ref_name,
                                callee_method,
                                call_range,
                                arg_ranges,
                            });
                        }

                        "instance_method_call" => {
                            // only handle relative-dot method calls with no chains for now for simplicity
                            let parts = get_node_children(do_arg);
                            if parts.len() != 1 {
                                eprintln!("Haven't yet implemented method calls with chains.");
                                generic_skipping_statements("build_method_calls", "Node", "Node");
                                continue;
                            }
                            let Some(rel) = parts.get(0) else {
                                eprintln!("Warning: failed to get index 0 from do parameter");
                                generic_skipping_statements("build_method_calls", "Node", "Node");
                                continue;
                            };
                            if rel.kind() != "relative_dot_method" {
                                eprintln!("Warning: expected node to be a relative dot method node, but it was a a {:?}", rel.kind());
                                generic_skipping_statements("build_method_calls", "Node", "Node");
                                continue;
                            }

                            let call_range = rel.range();

                            // oref_method node in your earlier code
                            let Some(oref_method) = rel.named_child(0) else {
                                eprintln!("Warning: expected node child to be an oref method node, but instead was None. Node is {:?}", rel);
                                generic_skipping_statements("build_method_calls", "Node", "Node");
                                continue;
                            };

                            let callee_method = {
                                let Some(m) = oref_method.named_child(0) else {
                                    eprintln!("Warning: expected node child to hold the callee method name, but instead was None. Node is {:?}", oref_method);
                                    generic_skipping_statements("build_method_calls", "Node", "Node");
                                    continue;
                                };
                                let Some(s) = get_string_at_byte_range(content, m.byte_range())
                                else {
                                    eprintln!("Warning: Failed to get string from content: {:?} for node: {:?}", content, m);
                                    continue;
                                };
                                s
                            };

                            let arg_ranges: Vec<Range> = oref_method
                                .named_child(1)
                                .map(|args_node| {
                                    get_node_children(args_node)
                                        .into_iter()
                                        .map(|a| a.range())
                                        .collect()
                                })
                                .unwrap_or_else(Vec::new);

                            out.push(UnresolvedCallSite {
                                callee_class: current_class.to_string(),
                                callee_method,
                                call_range,
                                arg_ranges,
                            });
                        }

                        _ => {
                            // ignore other DO forms for now
                            eprintln!("Warning: Unhandled DO command form for building method calls.");
                            generic_skipping_statements("build_method_calls", "Node", "Node");
                            continue;
                        }
                    }
                }

                "command_job" => {
                    // TODO: implement job statement parsing similarly
                    eprintln!("Warning: unhandled node command (JOB command) for building method calls.");
                    generic_skipping_statements("build_method_calls", "Node", "Node");
                    continue;
                }

                _ => {
                    eprintln!("Warning: unhandled node command {:?} for building method calls.", cmd.kind());
                    generic_skipping_statements("build_method_calls", "Node", "Node");
                    continue;
                }
            }
        }
    }
    successful_exit("COMMON (no struct)", "build_method_calls");
    out
}

/// Parses a `method_keywords` node and extracts semantic flags for a method.
///
/// Returns a tuple of:
/// - optional ProcedureBlock override (`Option<bool>`)
/// - optional Language override (`Option<Language>`)
/// - optional CodeMode (defaults to `Code`)
/// - `is_public` (defaults to `true` unless `Private` is present)
/// - list of declared public variables (from PublicList)
pub(crate) fn handle_method_keywords(
    node: Node,
    content: &str,
) -> Option<(
    Option<bool>,
    Option<Language>,
    Option<CodeMode>,
    bool,
    Vec<String>,
)> {
    start_of_function("COMMON: No struct", "handle_method_keywords");
    let mut is_procedure_block: Option<bool> = None;
    let mut is_public = true;
    let mut public_variables = Vec::new();
    let method_keywords_children = get_node_children(node.clone());
    let procedure_block = get_keyword("method_keyword", "procedure");
    let private_keyword = get_keyword("method_keyword", "private");
    let public_var_list = get_keyword("method_keyword", "public_list");
    let objectscript_language_keyword = get_keyword("method_keyword", "language");
    let external_language_keyword = "method_keyword_language".to_string();
    // regular codemode (core)
    let codemode_keyword = get_keyword("method_keyword", "codemode");
    // expression code mode (expression method)
    let expression_codemode_keyword = "method_keyword_codemode_expression".to_string();
    let call_codemode_keyword = "call_method_keyword".to_string();
    let mut codemode: Option<CodeMode> = None;
    let mut language: Option<Language> = None;
    // each node here is a class_keyword
    for node in method_keywords_children.iter() {
        let Some(keyword) = node.named_child(0) else {
            eprintln!("Warning: Expected method_keyword: {:?} to have a child at index 0, got None", node);
            generic_skipping_statements("handle_method_keywords", "Node", "Node");
            continue;
        };
        if keyword.kind() == procedure_block {
            if is_procedure_block.is_some() {
                eprintln!("Error: procedure block has already been set, cannot specify the same keyword twice.");
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
            let children = get_node_children(keyword.clone());
            if children.len() == 1 {
                is_procedure_block = Some(true);
            } else {
                let Some(rhs_keyword_node) = children.get(1) else {
                    eprintln!("Warning: failed to get rhs (index 1) of keyword node {:?}", children);
                    generic_skipping_statements("handle_method_keywords", "Node", "Node");
                    continue;
                };
                let Some(keyword_rhs) =
                    get_string_at_byte_range(content, rhs_keyword_node.byte_range())
                else {
                    eprintln!("Warning: failed to string content from content: {:?} of rhs of keyword", content);
                    generic_skipping_statements("handle_method_keywords", "Node", "Node");
                    continue;
                };
                match keyword_rhs.as_str() {
                    "0" => {
                        is_procedure_block = Some(false);
                    }
                    "1" => {
                        is_procedure_block = Some(true);
                    }
                    _ => {
                        eprintln!("Error: Can only set ProcedureBlock keyword to 0 or 1, not {:?}", keyword_rhs.as_str());
                        continue;
                    }
                }
            }
        } else if keyword.kind() == call_codemode_keyword {
            if codemode.is_some() {
                eprintln!("Error: CodeMode is already set, cannot specify the same keyword twice.");
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
            codemode = Some(CodeMode::Call);
        } else if keyword.kind() == expression_codemode_keyword {
            if codemode.is_some() {
                eprintln!("Error: CodeMode is already set, cannot specify the same keyword twice.");
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
            codemode = Some(CodeMode::Expression);
        } else if keyword.kind() == codemode_keyword {
            if codemode.is_some() {
                eprintln!("Error: CodeMode is already set, cannot specify the same keyword twice.");
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
            if let Some(value_node) = keyword.named_child(1) {
                if let Some(text) = content.get(value_node.byte_range()) {
                    if text.eq_ignore_ascii_case("code") {
                        codemode = Some(CodeMode::Code);
                    } else if text.eq_ignore_ascii_case("objectgenerator") {
                        codemode = Some(CodeMode::ObjectGenerator);
                    }
                    else {
                        eprintln!("Warning: For a method, the only acceptable keyword values are code and objectgenerator, not {:?}", text);
                        generic_skipping_statements("handle_method_keywords", "Node", "Node");
                        continue;
                    }
                }

                else {
                    eprintln!("Warning: failed to get string text from keyword value node.. {:?}", value_node);
                    generic_skipping_statements("handle_method_keywords", "Node", "Node");
                    continue;
                }
            }
            else {
                eprintln!("Warning: failed to get named child at index 1 for codemode keyword {:?}", keyword);
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
        } else if keyword.kind() == external_language_keyword {
            if language.is_some() {
                eprintln!("Error: Language is already set, cannot specify the same keyword twice.");
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
            if let Some(value_node) = keyword.named_child(1) {
                if let Some(text) = content.get(value_node.byte_range()) {
                    if text.eq_ignore_ascii_case("tsql") {
                        language = Some(Language::TSql);
                    } else if text.eq_ignore_ascii_case("python") {
                        language = Some(Language::Python);
                    } else if text.eq_ignore_ascii_case("ispl") {
                        language = Some(Language::ISpl);
                    } else {
                        eprintln!("For a method, the only acceptable keyword values for the Language keyword are tsql, python, objectscript, or ispl, not {:?}", text);
                        generic_skipping_statements("handle_method_keywords", "Node", "Node");
                        continue;
                    }
                }
            }
        } else if keyword.kind() == objectscript_language_keyword {
            if language.is_some() {
                eprintln!("Error: Language is already set, cannot specify the same keyword twice.");
                generic_skipping_statements("handle_method_keywords", "Node", "Node");
                continue;
            }
            language = Some(Language::Objectscript);
        } else if keyword.kind() == private_keyword {
            is_public = false;
        } else if keyword.kind() == public_var_list {
            let children = get_node_children(keyword.clone());
            for node in children.iter().skip(1) {
                if let Some(text) = content.get(node.byte_range()) {
                    public_variables.push(text.to_string());
                }
                else {
                    eprintln!("Error: failed to get string text from content {:?} for keyword child: {:?}", content, node);
                    generic_skipping_statements("handle_method_keywords", "Node", "Node");
                    continue;
                }
            }
        }
    }
    if codemode.is_none() {
        codemode = Some(CodeMode::Code);
    }
    successful_exit("COMMON: no struct", "handle_method_keywords");
    Some((
        is_procedure_block,
        language,
        codemode,
        is_public,
        public_variables,
    ))
}

pub fn successful_exit(struct_name: &str, function_name: &str) {
    eprintln!("Leaving {struct_name} function:{function_name}. Successfully reached the end");
    eprintln!("------------------------");
    eprintln!();
}

pub fn start_of_function(struct_name: &str, function_name: &str) {
    eprintln!("------------------------");
    eprintln!("In {struct_name} function: {function_name}...");
    eprintln!();
}

pub fn generic_exit_statements(struct_name: &str, function_name: &str) {
    eprintln!(
        "Aborting function early. Leaving {:?} function: {:?}",
        struct_name, function_name
    );
    eprintln!("------------------------");
    eprintln!();
}

pub(crate) fn generic_skipping_statements(function_name: &str, struct_name: &str, struct_type: &str) {
    eprintln!(
        "Skipping applying the logic from {function_name} to {struct_type} named {struct_name}"
    );
    eprintln!("------------------------");
    eprintln!();
}

/// Resolve a list of `UnresolvedCallSite`s into concrete `MethodCallSite`s.
///
/// Uses `classes_map` to map callee class names to `ClassId`, then uses `idx.effective_public_methods`
/// to resolve the callee method to a `PublicMethodRef` (if known and public).
///
/// Any call that cannot be resolved remains with `callee_symbol = None`. The returned call sites
/// retain the original `call_range` and `arg_ranges` for later navigation/highlighting.
pub fn build_method_calls_from_unresolved(
    classes_map: HashMap<String, ClassId>,
    idx: OverrideIndex,
    unresolved_call_site: Vec<UnresolvedCallSite>,
    method_name: String,
) -> Vec<MethodCallSite> {
    start_of_function("Not part of struct", "build_method_calls_from_unresolved");
    let new_sites: Vec<MethodCallSite> = unresolved_call_site
        .into_iter()
        .map(|call| {
            let callee_symbol = classes_map
                .get(&call.callee_class)
                .copied()
                .and_then(|callee_class_id| idx.effective_public_methods.get(&callee_class_id))
                .and_then(|tbl| tbl.get(&call.callee_method).copied());

            MethodCallSite {
                caller_method: method_name.clone(),
                callee_class: call.callee_class,
                callee_method: call.callee_method,
                callee_symbol,
                call_range: call.call_range,
                arg_ranges: call.arg_ranges,
            }
        })
        .collect();
    successful_exit("Not part of struct", "build_method_calls_from_unresolved");
    new_sites
}
