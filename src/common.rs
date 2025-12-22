use serde_json::Value;
use tree_sitter::{Node, Point, Tree};
use crate::parse_structures::{ReturnType, VarType};
use crate::scope_tree::{ScopeTree};
use crate::scope_structures::ScopeId;

pub fn get_node_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect::<Vec<Node>>()
}

// given root node, gets the class name
pub fn get_class_name_from_root(content:&str,node:Node) -> String {
    content[node.named_child(node.named_child_count() - 1).unwrap().named_child(1).unwrap().byte_range()].to_string()
}

/// given an expr atom node, return the var type.
pub fn get_expr_atom_var_type(node: Node) -> Option<VarType> {
    let node = node.child(0).unwrap();
    match node.kind() {
        "json_object_literal" => Some(VarType::JsonObjectLiteral),
        "json_array_literal" => Some(VarType::JsonArrayLiteral),
        "_parenthetical_expression" => {
            let expression = node.named_child(0).unwrap();
            find_var_type_from_expression(expression)
        }
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
        _ => {
            // TODO: unary_expression, macro, variables (lvn, ssvn, gvn, instance_variable, sql_field_reference)
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
/// helper function to try to find the var type given an expression node
/// For simplification, only checks the expr_atom, but future iterations
/// TODO will also check the expr_tails
pub fn find_var_type_from_expression(node: Node) -> Option<VarType> {
    let children = get_node_children(node);
    return get_expr_atom_var_type(children[0]);
    // if children.len() > 1 {
    //
    // }
    // else {
    //     return get_expr_atom_var_type(children[0]);
    // }
    // None
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


fn build_scope_skeleton(node: Node, scope_tree: &mut ScopeTree, scope_stack: &mut Vec<ScopeId>) {
    let is_scope = cls_is_scope_node(node);

    if is_scope {
        let parent = *scope_stack.last().unwrap();
        let scope_id = scope_tree.add_scope(
            node.start_position(),
            node.end_position(),
            parent,
            None,
            false,
        );
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
