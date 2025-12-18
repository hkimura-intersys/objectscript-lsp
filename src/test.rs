#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tower_lsp::lsp_types::Url;
    use tree_sitter::Parser;
    use tree_sitter_objectscript::LANGUAGE_OBJECTSCRIPT;

    use crate::document::Document;
    use crate::parse_structures::{FileType, Language};
    use crate::workspace::ProjectState;

    fn parse_cls(code: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE_OBJECTSCRIPT.into())
            .expect("failed to load objectscript grammar");
        parser.parse(code, None).expect("parse returned None")
    }

    #[test]
    fn local_semantics_parses_extends_into_inherited_classes() {
        let code = r#"
Class Barricade Extends MyOtherCls
{
ClassMethod Parent()
{
    quit
}
}
"#;

        let tree = parse_cls(code);
        let url = Url::parse("file:///test/Barricade.cls").unwrap();

        let doc = Document::new(code.to_string(), tree, FileType::Cls, url.clone());

        let project = ProjectState::new();
        project.add_document(url.clone(), doc);

        let docs = project.documents.read();
        let d = docs.get(&url).unwrap();
        let local = d.local_semantic_model.as_ref().expect("missing LocalSemanticModel");

        assert_eq!(local.class.name, "Barricade");
        assert_eq!(
            local.class.inherited_classes,
            vec!["MyOtherCls".to_string()],
            "expected inherited_classes to contain MyOtherCls"
        );
    }

    #[test]
    fn project_state_builds_scope_tree_and_updates_global_subclasses() {
        let b_code = r#"
Class MyApp.B
{
ClassMethod Parent()
{
    quit
}
}
"#;

        let a_code = r#"
Class MyApp.A Extends MyApp.B
{
Method Child()
{
    quit
}
}
"#;

        let b_tree = parse_cls(b_code);
        let a_tree = parse_cls(a_code);

        let b_url = Url::parse("file:///test/MyApp/B.cls").unwrap();
        let a_url = Url::parse("file:///test/MyApp/A.cls").unwrap();

        let b_doc = Document::new(b_code.to_string(), b_tree, FileType::Cls, b_url.clone());
        let a_doc = Document::new(a_code.to_string(), a_tree, FileType::Cls, a_url.clone());

        let project = ProjectState::new();
        project.add_document(b_url.clone(), b_doc);
        project.add_document(a_url.clone(), a_doc);

        // 1) Scope test: root + 1 method/classmethod scope
        {
            let defs = project.defs.read();
            let b_scopes = defs.get(&b_url).expect("missing defs for B");
            let a_scopes = defs.get(&a_url).expect("missing defs for A");

            assert_eq!(b_scopes.scopes.read().len(), 2, "B should have root + 1 method scope");
            assert_eq!(a_scopes.scopes.read().len(), 2, "A should have root + 1 method scope");
        }

        // 2) Local semantic test: extends is parsed automatically
        {
            let docs = project.documents.read();
            let a = docs.get(&a_url).unwrap();
            let a_local = a.local_semantic_model.as_ref().unwrap();

            assert_eq!(a_local.class.name, "MyApp.A");
            assert_eq!(a_local.class.inherited_classes, vec!["MyApp.B".to_string()]);
        }

        // 3) Global update test: subclasses should already be populated by add_document()
        {
            let g = project.global_semantic_model.read();
            let subs = g.subclasses.get("MyApp.B").cloned().unwrap_or_default();
            let got: HashSet<_> = subs.into_iter().collect();

            assert!(
                got.contains("MyApp.A"),
                "expected subclasses[MyApp.B] to contain MyApp.A; got {got:?}"
            );
        }
    }

    #[test]
    fn project_state_keyword_inheritance() {
        let b_code = r#"
        Class MyApp.B Extends MyApp.D [Language = tsql]
        {
        ClassMethod Parent()
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
        project.add_document(b_url.clone(), b_doc);
        project.add_document(a_url.clone(), a_doc);
        project.add_document(c_url.clone(), c_doc);
        project.add_document(d_url.clone(), d_doc);
        project.add_document(e_url.clone(), e_doc);

        project.global_update_inherited_classes();
        println!("FINISHED");
        // doc a inherits doc b which inherits doc d. So, doc b keywords are Not ProcedureBlock, and then the language = tsql
        // doc a doesn't specify any of these, so keywords should be the same as doc b
        let docs = project.documents.read();
        let doc_a = docs.get(&a_url).unwrap();
        let inheritance_direction_a = doc_a.local_semantic_model.clone().unwrap().class.inheritance_direction;
        let is_procedure_block_a = doc_a.local_semantic_model.clone().unwrap().class.is_procedure_block.unwrap();
        let language_a = doc_a.local_semantic_model.clone().unwrap().class.default_language.unwrap();
        assert_eq!(inheritance_direction_a, "right".to_string());
        assert_eq!(is_procedure_block_a, false);
        assert_eq!(language_a, Language::TSql);

        // inherits a doc, but already has the keywords specified. Expected: SHOULD NOT INHERIT THESE KEYWORDS
        let doc_e = docs.get(&e_url).unwrap();
        let inheritance_direction_e = doc_e.local_semantic_model.clone().unwrap().class.inheritance_direction;
        let is_procedure_block_e = doc_e.local_semantic_model.clone().unwrap().class.is_procedure_block.unwrap();
        let language_e = doc_e.local_semantic_model.clone().unwrap().class.default_language.unwrap();
        assert_eq!(inheritance_direction_e, "left".to_string());
        assert_eq!(is_procedure_block_e, true);
        assert_eq!(language_e, Language::Objectscript);
        drop(docs);
    }
}
