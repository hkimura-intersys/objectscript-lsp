#[cfg(test)]
mod tests {
    use crate::common::get_class_name_from_root;

    use tower_lsp::lsp_types::Url;
    use tree_sitter::Parser;
    use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};

    use crate::document::Document;
    use crate::parse_structures::{FileType, Language};
    use crate::common::get_keyword;
    use crate::workspace::ProjectState;

    fn parse_cls(code: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE_OBJECTSCRIPT.into())
            .expect("failed to load objectscript grammar");
        parser.parse(code, None).expect("parse returned None")
    }

    fn parse_routine(code: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE_OBJECTSCRIPT_CORE.into())
            .expect("failed to load objectscript grammar");
        parser.parse(code, None).expect("parse returned None")
    }
    #[test]
    fn test_parsing_routine() {
        let code = r#"
          set x = {2}
        "#;
        let tree = parse_routine(code);
        let command_set_rhs = tree
            .root_node()
            .named_child(0)
            .unwrap()
            .named_child(0)
            .unwrap()
            .named_child(0)
            .unwrap()
            .named_child(1)
            .unwrap()
            .named_child(1)
            .unwrap();

        println!("{:#?}", command_set_rhs.named_child(0));
    }

    #[test]
    fn project_state_keyword_inheritance() {
        let b_code = r#"
        Class MyApp.B Extends MyApp.D [Language = tsql]
        {
        ClassMethod Parent() [Private]
        {
            quit
        }
        }
        "#;

        let c_code = r#"
        Class MyApp.C [Not ProcedureBlock]
        {
        ClassMethod Parent()
        {
            quit
        }
        }
        "#;

        let a_code = r#"
        Class MyApp.A Extends (MyApp.B, MyApp.C) [inheritance = right]
        {
        Method Child()
        {
            quit
        }
        }
        "#;

        let d_code = r#"
        Class MyApp.D [Not ProcedureBlock]
        {
        Method Child()
        {
            quit
        }
        }
        "#;

        let e_code = r#"
        Class MyApp.E Extends D [ProcedureBlock,Language=objectscript]
        {
        Method Child()
        {
            quit
        }
        }
        "#;

        // even with right inheritance, it should only inherit the keywords of the leftmost superclass
        // expected: ProcedureBlock, Lanugage = tsql
        let b_tree = parse_cls(b_code);
        let a_tree = parse_cls(a_code);
        let c_tree = parse_cls(c_code);
        let d_tree = parse_cls(d_code);
        let e_tree = parse_cls(e_code);

        let a_class_name = get_class_name_from_root(a_code, a_tree.root_node());
        let b_class_name = get_class_name_from_root(b_code, b_tree.root_node());
        let c_class_name = get_class_name_from_root(c_code, c_tree.root_node());
        let d_class_name = get_class_name_from_root(d_code, d_tree.root_node());
        let e_class_name = get_class_name_from_root(e_code, e_tree.root_node());

        let b_url = Url::parse("file:///test/MyApp/B.cls").unwrap();
        let a_url = Url::parse("file:///test/MyApp/A.cls").unwrap();
        let c_url = Url::parse("file:///test/MyApp/C.cls").unwrap();
        let d_url = Url::parse("file:///test/MyApp/D.cls").unwrap();
        let e_url = Url::parse("file:///test/MyApp/E.cls").unwrap();

        let b_doc = Document::new(b_code.to_string(), b_tree, FileType::Cls, b_url.clone());
        let a_doc = Document::new(a_code.to_string(), a_tree, FileType::Cls, a_url.clone());
        let c_doc = Document::new(c_code.to_string(), c_tree, FileType::Cls, c_url.clone());
        let d_doc = Document::new(d_code.to_string(), d_tree, FileType::Cls, d_url.clone());
        let e_doc = Document::new(e_code.to_string(), e_tree, FileType::Cls, e_url.clone());

        let project = ProjectState::new();

        project.add_document(b_url.clone(), b_doc, a_class_name);
        project.add_document(a_url.clone(), a_doc, b_class_name);
        project.add_document(c_url.clone(), c_doc, c_class_name);
        project.add_document(d_url.clone(), d_doc, d_class_name);
        project.add_document(e_url.clone(), e_doc, e_class_name);

        let local_semantic_models = project.local_semantic_models.read();
        let global_semantic_model = project.global_semantic_model.read();
        // println!("LOCAL SEMANTIC MODELS {:#?}", local_semantic_models);
        let a_local_semantic_model = project.local_semantic_models.read().get(&a_url).unwrap().clone();
        let a_methods = project.global_semantic_model.read().private[a_local_semantic_model.0].clone();
        assert_eq!(a_methods.methods.len(), 0);

        let b_local_semantic_model = project.local_semantic_models.read().get(&b_url).unwrap().clone();
        let b_methods = project.global_semantic_model.read().private[b_local_semantic_model.0].clone();
        assert_eq!(b_methods.methods.len(), 1);


        // assert_eq!( None, local_semantic_models.get(&a_url));
        println!("{:?}", local_semantic_models.get(&b_url));
        // assert_eq!( None, local_semantic_models.get(&c_url));
        // assert_eq!( None, local_semantic_models.get(&d_url));
        // assert_eq!( None, local_semantic_models.get(&e_url));



        // println!("FINISHED");
        // // doc a inherits doc b which inherits doc d. So, doc b keywords are Not ProcedureBlock, and then the language = tsql
        // // doc a doesn't specify any of these, so keywords should be the same as doc b
        // let docs = project.documents.read();
        // let doc_a = docs.get(&a_url).unwrap();
        // let inheritance_direction_a = doc_a
        //     .local_semantic_model
        //     .clone()
        //     .unwrap()
        //     .class
        //     .inheritance_direction;
        // let is_procedure_block_a = doc_a
        //     .local_semantic_model
        //     .clone()
        //     .unwrap()
        //     .class
        //     .is_procedure_block
        //     .unwrap();
        // let language_a = doc_a
        //     .local_semantic_model
        //     .clone()
        //     .unwrap()
        //     .class
        //     .default_language
        //     .unwrap();
        // assert_eq!(inheritance_direction_a, "right".to_string());
        // assert_eq!(is_procedure_block_a, false);
        // assert_eq!(language_a, Language::TSql);
        //
        // // inherits a doc, but already has the keywords specified. Expected: SHOULD NOT INHERIT THESE KEYWORDS
        // let doc_e = docs.get(&e_url).unwrap();
        // let inheritance_direction_e = doc_e
        //     .local_semantic_model
        //     .clone()
        //     .unwrap()
        //     .class
        //     .inheritance_direction;
        // let is_procedure_block_e = doc_e
        //     .local_semantic_model
        //     .clone()
        //     .unwrap()
        //     .class
        //     .is_procedure_block
        //     .unwrap();
        // let language_e = doc_e
        //     .local_semantic_model
        //     .clone()
        //     .unwrap()
        //     .class
        //     .default_language
        //     .unwrap();
        // assert_eq!(inheritance_direction_e, "left".to_string());
        // assert_eq!(is_procedure_block_e, true);
        // assert_eq!(language_e, Language::Objectscript);
        // drop(docs);
    }
}
