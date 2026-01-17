#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::common::{point_to_byte, is_set_argument_parent, method_name_from_identifier_node};
    use crate::document::Document;
    use crate::parse_structures::{FileType, Language};
    use crate::workspace::ProjectState;
    use tower_lsp::lsp_types::Url;
    use tree_sitter::{Parser, Point};
    use tree_sitter_objectscript::LANGUAGE_OBJECTSCRIPT;

    fn parse_cls(code: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE_OBJECTSCRIPT.into())
            .expect("failed to load objectscript grammar");
        parser.parse(code, None).expect("parse returned None")
    }

//     #[test]
//     fn test_parsing() {
//         let a_code = r#"
// Class MyApp
// {
// Method Child()
// {
//     do ..test()
// }
// }
// "#;
//
//         let point = Point::new(3, 10);
//         let point_2 = Point::new(3, 11);
//         let tree = parse_cls(a_code);
//         let node = tree
//             .root_node()
//             .named_descendant_for_point_range(point, point)
//             .unwrap();
//         let byte = point_to_byte(a_code, point);
//         let second_byte = point_to_byte(a_code, point_2);
//         let string = a_code[node.byte_range()].to_string();
//         println!("{:?}", string);
//         println!("{:?}", node.parent().unwrap().parent().unwrap().kind());
//         println!("{}", tree.root_node().to_sexp());
//     }

    #[test]
    fn find_var_nodes() {
        let a_code = r#"
        Class MyApp
        {
        Method Child()
        {
            set x = test + test2
        }
        }
        "#;
        let point = Point::new(5, 29);
        let point_2 = Point::new(5, 30);
        let tree = parse_cls(a_code);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
            .unwrap();
        let byte = point_to_byte(a_code, point);
        let second_byte = point_to_byte(a_code, point_2);
        let string = a_code[node.byte_range()].to_string();
        println!("{:?}", string);
        println!("{:?}", node);
        let is_set_arg = is_set_argument_parent(node, 0);
        println!("{}", is_set_arg);
        let method_name = method_name_from_identifier_node(node, a_code, 0).unwrap();
        println!("{:?}", method_name);
        // println!("{:?}", node.parent().unwrap().parent().unwrap().kind());
        // println!("{}", tree.root_node().to_sexp());
    }
}
