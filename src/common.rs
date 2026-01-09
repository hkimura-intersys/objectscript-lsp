use crate::parse_structures::{ReturnType, VarType};
use crate::scope_structures::ScopeId;
use crate::scope_tree::ScopeTree;
use serde_json::Value;
use tower_lsp::lsp_types::Position;
use tree_sitter::{Node, Point, Tree};

pub fn point_to_byte(text: &str, point: Point) -> usize {
    let mut row = 0usize;
    let mut offset = 0usize;

    for (i, c) in text.char_indices() {
        if row == point.row {
            offset = i;
            break;
        }
        if c == '\n' {
            row += 1;
        }
    }

    offset + point.column
}


pub fn position_to_point(
    text: &str,
    position: Position,
) -> Point {
    let mut row = 0usize;
    let mut line_start = 0usize;

    // find start of the target line
    for (i, c) in text.char_indices() {
        if row == position.line as usize {
            line_start = i;
            break;
        }
        if c == '\n' {
            row += 1;
        }
    }

    // Convert UTF-16 character offset → byte offset
    let mut utf16_units = 0u32;
    let mut column_bytes = 0usize;

    for c in text[line_start..].chars() {
        if utf16_units >= position.character {
            break;
        }
        utf16_units += c.len_utf16() as u32;
        column_bytes += c.len_utf8();
    }

    Point {
        row: position.line as usize,
        column: column_bytes,
    }
}

pub fn advance_point(mut row: usize, mut column: usize, changed_text: &str) -> Point {
    for c in changed_text.chars() {
        if c == '\n' {
            row += 1;
            column = 0;
        } else {
            column += c.len_utf8();
        }
    }

    Point { row, column }
}

pub fn get_node_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect::<Vec<Node>>()
}

// given root node, gets the class name
pub fn get_class_name_from_root(content: &str, node: Node) -> String {
    content[node
        .named_child(node.named_child_count() - 1)
        .unwrap()
        .named_child(1)
        .unwrap()
        .byte_range()]
    .to_string()
}

/// given an expr atom node, return the var type.
pub fn get_expr_atom_var_type(node: Node, content: &str) -> Option<VarType> {
    let node = node.named_child(0).unwrap();
    match node.kind() {
        "json_object_literal" => Some(VarType::JsonObjectLiteral),
        "json_array_literal" => Some(VarType::JsonArrayLiteral),
        "string_literal" => Some(VarType::String),
        "numeric_literal" => Some(VarType::Number),
        "relative_dot_method" => Some(VarType::RelativeDotMethod),
        "relative_dot_property" => Some(VarType::RelativeDotProperty),
        "relative_dot_parameter" => Some(VarType::RelativeDotParameter),
        "oref_chain_expr" => {
            // either a method call or
            Some(VarType::OrefChainExpr)
        }
        "class_method_call" => Some(VarType::ClassMethodCall),
        "class_parameter_ref" => Some(VarType::ClassParameterRef),
        "superclass_method_call" => Some(VarType::SuperclassMethodCall),
        "gvn" => {
            let var_name = content[node.byte_range()].to_string();
            Some(VarType::Gvn(var_name))
        }
        "lvn" => {
            let var_name = content[node.byte_range()].to_string();
            Some(VarType::Lvn(var_name))
        }
        "instance_variable" => {
            let property_name = content[node.named_child(0).unwrap().byte_range()].to_string();
            Some(VarType::InstanceVariable(property_name))
        }
        _ => {
            // TODO: macro, ssvn, sql_field_reference
            println!("Unimplemented: {:?}", node.kind());
            None
        }
    }
}

/// given a typename node, find the corresponding ReturnType
pub fn find_return_type(typename: String) -> Option<ReturnType> {
    match typename.as_str() {
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
    }
}

/// helper function to try to find the var types given an expression node
pub fn find_var_type_from_expression(node: Node, content: &str) -> Vec<VarType> {
    let mut var_types = Vec::new();
    let children = get_node_children(node);
    if children[0].kind() == "_parenthetical_expression" {
        let expression = children[0].named_child(0).unwrap();
        let result = find_var_type_from_expression(expression, content);
        for v in result {
            var_types.push(v);
        }
    } else if children[0].kind() == "expression" {
        let result = find_var_type_from_expression(children[0], content);
        for v in result {
            var_types.push(v);
        }
    } else if children[0].kind() == "unary_expression" {
        let unary_child = children[0].named_child(0).unwrap();
        if unary_child.kind() == "expression" {
            let result = find_var_type_from_expression(unary_child, content);
            for v in result {
                var_types.push(v);
            }
        } else if unary_child.kind() == "glvn" {
            let var_name = content[unary_child.byte_range()].to_string();
            var_types.push(VarType::Gvn(var_name));
        }
    } else {
        let expr_atom_type = get_expr_atom_var_type(children[0], content);
        if expr_atom_type.is_some() {
            var_types.push(expr_atom_type.unwrap());
        }
    }

    for node in children[1..].iter() {
        // each node is an expr tail
        if node.named_child(0).unwrap().kind() == "binary_operator" {
            // binary operator + expression case
            let var_types_expr_tail =
                find_var_type_from_expression(node.named_child(1).unwrap(), content);
            for v in var_types_expr_tail {
                var_types.push(v);
            }
        } else {
            println!("Unimplemented expr atom type: {:?}", node.kind());
            continue;
        }
    }
    var_types
}

pub fn get_keyword(keyword_type: &str, filter: &str) -> String {
    let json = tree_sitter_objectscript::OBJECTSCRIPT_NODE_TYPES; // &'static str
    let v: Value = serde_json::from_str(json).expect("invalid node-types.json");

    // node-types.json is an array of objects
    let arr = v.as_array().expect("node-types.json must be a JSON array");

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
                        return ty.to_string();
                    }
                }
            }
        }
    }
    "".to_string()
}

pub fn initial_build_scope_tree(tree: Tree) -> ScopeTree {
    let mut scope_tree = ScopeTree::new();
    let mut scope_stack = vec![scope_tree.root];

    let root = tree.root_node();
    build_scope_skeleton(root, &mut scope_tree, &mut scope_stack);

    scope_tree
}

pub fn node_affects_class_level(kind: &str) -> bool {
    matches!(kind,
        "class_keywords" | "class_extends" | "import_code" |
        "method_definition" | "class_body"
    )
}

fn build_scope_skeleton(node: Node, scope_tree: &mut ScopeTree, scope_stack: &mut Vec<ScopeId>) {
    let is_scope = cls_is_scope_node(node);

    if is_scope {
        let parent = *scope_stack.last().unwrap();
        let scope_id =
            scope_tree.add_scope(node.start_position(), node.end_position(), parent, false);
        scope_stack.push(scope_id);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        build_scope_skeleton(child, scope_tree, scope_stack);
    }

    if is_scope {
        scope_stack.pop();
    }
}

pub fn point_in_range(pos: Point, start: Point, end: Point) -> bool {
    if pos >= start && pos < end {
        return true;
    };
    false
}

pub fn cls_is_scope_node(node: Node) -> bool {
    node.kind() == "classmethod" || node.kind() == "method"
}
