#[cfg(test)]
mod tests {
    use crate::common::get_class_name_from_root;

    use tower_lsp::lsp_types::Url;
    use tree_sitter::Parser;
    use tree_sitter_objectscript::{LANGUAGE_OBJECTSCRIPT, LANGUAGE_OBJECTSCRIPT_CORE};
    use crate::document::Document;
    use std::collections::HashMap;
    use crate::parse_structures::{Class, ClassId, CodeMode, FileType, GlobalSemanticModel, Language, Method, MethodId, MethodType};
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
          set x = 2
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
        let a_local_semantic_model = project
            .local_semantic_models
            .read()
            .get(&a_url)
            .unwrap()
            .clone();
        let a_methods =
            project.global_semantic_model.read().private[a_local_semantic_model.0].clone();
        assert_eq!(a_methods.methods.len(), 0);

        let b_local_semantic_model = project
            .local_semantic_models
            .read()
            .get(&b_url)
            .unwrap()
            .clone();
        let b_methods =
            project.global_semantic_model.read().private[b_local_semantic_model.0].clone();
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

    fn mk_public_method(name: &str) -> Method {
        Method::new(
            name.to_string(),
            None,                    // is_procedure_block
            Some(Language::Objectscript),
            CodeMode::Code,
            true,                    // is_public
            None,                    // return_type
            vec![],                  // public_variables_declared
            MethodType::ClassMethod, // doesn't matter for override tests
        )
    }

    fn add_class_with_public_methods(
        gsm: &mut GlobalSemanticModel,
        name: &str,
        parents: Vec<ClassId>,
        inheritance_direction: &str, // "left" | "right"
        method_names: &[&str],
    ) -> (ClassId, HashMap<String, MethodId>) {
        // add methods first
        let mut pub_map: HashMap<String, MethodId> = HashMap::new();
        for &m in method_names {
            let id = gsm.new_method(mk_public_method(m));
            pub_map.insert(m.to_string(), id);
        }

        let mut cls = Class::new(name.to_string());
        cls.inherited_classes = parents;
        cls.inheritance_direction = inheritance_direction.to_string();
        cls.public_methods = pub_map.clone();

        let cid = gsm.new_class(cls);
        (cid, pub_map)
    }

    #[test]
    fn override_simple_child_overrides_parent() {
        let mut gsm = GlobalSemanticModel::new();

        let (_b_id, b_methods) =
            add_class_with_public_methods(&mut gsm, "B", vec![], "left", &["test_write"]);
        let (_a_id, a_methods) = add_class_with_public_methods(
            &mut gsm,
            "A",
            vec![ClassId(0)], // A extends B
            "left",
            &["test_write"],
        );

        let idx = gsm.build_override_index_public_only();

        let b_mid = *b_methods.get("test_write").unwrap();
        let a_mid = *a_methods.get("test_write").unwrap();

        // A overrides B
        assert_eq!(
            idx.overrides.get(&a_mid).copied(),
            Some(b_mid),
            "expected A.test_write to override B.test_write"
        );

        // effective dispatch on A is A's method
        assert_eq!(
            idx.effective_public_methods
                .get(&ClassId(1))
                .and_then(|m| m.get("test_write"))
                .copied(),
            Some(a_mid),
            "expected effective A.test_write == A.test_write"
        );

        // and on B it's B's
        assert_eq!(
            idx.effective_public_methods
                .get(&ClassId(0))
                .and_then(|m| m.get("test_write"))
                .copied(),
            Some(b_mid),
            "expected effective B.test_write == B.test_write"
        );

        // reverse index
        assert!(
            idx.overridden_by.get(&b_mid).map(|v| v.contains(&a_mid)).unwrap_or(false),
            "expected overridden_by[B.test_write] to include A.test_write"
        );
    }

    #[test]
    fn precedence_left_vs_right_affects_which_base_is_overridden() {
        // B and C both define test_write. A extends [B, C] and defines test_write.
        // With left inheritance: base should be B.test_write
        // With right inheritance: base should be C.test_write

        // ---- left case ----
        {
            let mut gsm = GlobalSemanticModel::new();
            let (_b, b_methods) =
                add_class_with_public_methods(&mut gsm, "B", vec![], "left", &["test_write"]);
            let (_c, c_methods) =
                add_class_with_public_methods(&mut gsm, "C", vec![], "left", &["test_write"]);

            let (_a, a_methods) = add_class_with_public_methods(
                &mut gsm,
                "A",
                vec![ClassId(0), ClassId(1)], // [B, C]
                "left",
                &["test_write"],
            );

            let idx = gsm.build_override_index_public_only();
            let a_mid = *a_methods.get("test_write").unwrap();
            let b_mid = *b_methods.get("test_write").unwrap();
            let c_mid = *c_methods.get("test_write").unwrap();

            assert_eq!(
                idx.overrides.get(&a_mid).copied(),
                Some(b_mid),
                "left inheritance: expected A.test_write to override B.test_write (not C)"
            );
            assert_ne!(
                idx.overrides.get(&a_mid).copied(),
                Some(c_mid),
                "left inheritance: should not override C's method"
            );
        }

        // ---- right case ----
        {
            let mut gsm = GlobalSemanticModel::new();
            let (_b, b_methods) =
                add_class_with_public_methods(&mut gsm, "B", vec![], "left", &["test_write"]);
            let (_c, c_methods) =
                add_class_with_public_methods(&mut gsm, "C", vec![], "left", &["test_write"]);

            let (_a, a_methods) = add_class_with_public_methods(
                &mut gsm,
                "A",
                vec![ClassId(0), ClassId(1)], // [B, C]
                "right",
                &["test_write"],
            );

            let idx = gsm.build_override_index_public_only();
            let a_mid = *a_methods.get("test_write").unwrap();
            let b_mid = *b_methods.get("test_write").unwrap();
            let c_mid = *c_methods.get("test_write").unwrap();

            assert_eq!(
                idx.overrides.get(&a_mid).copied(),
                Some(c_mid),
                "right inheritance: expected A.test_write to override C.test_write (not B)"
            );
            assert_ne!(
                idx.overrides.get(&a_mid).copied(),
                Some(b_mid),
                "right inheritance: should not override B's method"
            );
        }
    }

    #[test]
    fn transitive_inherited_method_is_used_for_dispatch() {
        // E defines foo
        // B extends E (does not define foo)
        // A extends [B] (does not define foo)
        // => effective A.foo should be E.foo

        let mut gsm = GlobalSemanticModel::new();

        let (_e, e_methods) = add_class_with_public_methods(&mut gsm, "E", vec![], "left", &["foo"]);
        let (_b, _b_methods) = add_class_with_public_methods(
            &mut gsm,
            "B",
            vec![ClassId(0)], // B extends E
            "left",
            &[],              // B doesn't define foo
        );
        let (_a, _a_methods) = add_class_with_public_methods(
            &mut gsm,
            "A",
            vec![ClassId(1)], // A extends B
            "left",
            &[],              // A doesn't define foo
        );

        let idx = gsm.build_override_index_public_only();

        let e_mid = *e_methods.get("foo").unwrap();

        assert_eq!(
            idx.effective_public_methods
                .get(&ClassId(2)) // A is index 2
                .and_then(|m| m.get("foo"))
                .copied(),
            Some(e_mid),
            "expected effective A.foo to come from E.foo transitively"
        );
    }

    #[test]
    #[should_panic(expected = "Cycle detected")]
    fn cycle_panics() {
        // A extends B, B extends A
        let mut gsm = GlobalSemanticModel::new();

        let (_a, _a_methods) = add_class_with_public_methods(&mut gsm, "A", vec![], "left", &[]);
        let (_b, _b_methods) = add_class_with_public_methods(&mut gsm, "B", vec![ClassId(0)], "left", &[]);

        // mutate A to extend B (creates A<->B cycle)
        gsm.classes[0].inherited_classes = vec![ClassId(1)];

        let _idx = gsm.build_override_index_public_only();
    }
}
