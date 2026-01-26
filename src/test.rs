#[cfg(test)]
mod tests {
    use crate::backend_testing::BackendTester;
    use crate::parse_structures::Language;
    use crate::workspace::ProjectState;
    use std::env;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::Url;
    use tree_sitter::Parser;
    use tree_sitter_objectscript::LANGUAGE_OBJECTSCRIPT;

    fn parse_cls(code: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE_OBJECTSCRIPT.into())
            .expect("failed to load objectscript grammar");
        parser.parse(code, None).expect("parse returned None")
    }

    async fn setup_backend_and_workspace(project_root: PathBuf) -> (BackendTester, Url) {
        let project_state = ProjectState::new();
        // create projectState and set the projectRoot
        let state = ProjectState::new();
        if state
            .project_root_path
            .set(Some(project_root.clone()))
            .is_err()
        {
            eprintln!("failed to set the root path");
        }
        let backend = BackendTester::new();
        let uri = Url::from_file_path(project_root).unwrap();
        backend.add_project(uri.clone(), state);
        // println!("{:#?}", backend);

        let _ = backend.index_workspace(&uri).await;
        (backend, uri)
    }

    #[tokio::test]
    // TODO: move from print statements to an actual test
    async fn test_variables() {
        // let project_root = env::current_dir().unwrap().join("objectscript-tests").join("variables");
        let project_root = env::current_dir()
            .unwrap()
            .join("objectscript-tests")
            .join("inheritance")
            .join("class-keyword-inheritance.cls");
        let (backend, uri) = setup_backend_and_workspace(project_root).await;
        let project_state = backend.get_project(&uri).unwrap();
        let project_data = project_state.data.read();
        let project_gsm_variable_defs = project_data.global_semantic_model.variable_defs.clone();
        let project_public_variables = project_data.pub_var_defs.clone();
        let project_documents = project_data.documents.clone();

        println!("PUB VARS AFTER INDEXING {:#?}", project_public_variables);
        println!("AFTER INDEXING {:#?}", backend);
        for (var_name, class_name_to_variable_symbol_ids) in project_public_variables {
            println!("GETTING SYMBOLS FOR VARIABLE: {:?}", var_name);
            for (class_name, variable_symbol_ids) in class_name_to_variable_symbol_ids {
                println!("CURRENT CLASS: {:?}", class_name);
                let class_global_symbol_id = project_data.class_defs.get(&class_name).unwrap();
                let variable_symbols_for_class = project_gsm_variable_defs
                    .get(class_global_symbol_id)
                    .unwrap();
                for variable_symbol_id in variable_symbol_ids {
                    let variable_symbol = variable_symbols_for_class
                        .get(variable_symbol_id.0)
                        .unwrap();
                    let document = project_documents.get(&variable_symbol.url).unwrap();
                    let content = document.content.as_str();
                    let start_byte = variable_symbol.location.start_byte;
                    let end_byte = variable_symbol.location.end_byte;
                    let start_point = variable_symbol.location.start_point;
                    let end_point = variable_symbol.location.end_point;
                    println!("START POINT: {:?}, END POINT: {:?}", start_point, end_point);
                    let byte_range = std::ops::Range {
                        start: start_byte,
                        end: end_byte,
                    };
                    println!(
                        "THE CORRESPONDING CONTENT FOR VARIABLE SYMBOL: {:?}",
                        content.get(byte_range)
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_class_keyword_inheritance() {
        // KEYWORDS: language = objectscript, inheritance = right, Not ProcedureBlock
        let project_root = env::current_dir()
            .unwrap()
            .join("objectscript-tests")
            .join("inheritance")
            .join("class-keyword-inheritance.cls");
        let (backend, uri) = setup_backend_and_workspace(project_root).await;
        let project_state = backend.get_project(&uri).unwrap();
        let project_data = project_state.data.read();
        let classes = project_data.classes.clone();
        let gsm = project_data.global_semantic_model.clone();
        for (class_name, class_id) in classes {
            let class = &gsm.classes[class_id.0];
            assert_eq!(class.is_procedure_block, Some(false));
            assert_eq!(class.default_language, Some(Language::Objectscript));
            assert_eq!(class.inheritance_direction, "right");
            let methods_in_class = gsm.methods.get(&class_id).unwrap();
            // get methods
            for (method_name, pub_method_id) in class.public_methods.clone() {
                let method = methods_in_class[pub_method_id.0].clone();
                if method.name == "newVarChange" {
                    assert_eq!(method.private_variables.len(), 1);
                    assert_eq!(method.public_variables.len(), 0);
                    assert_eq!(method.is_procedure_block, Some(true));
                    assert_eq!(method.language, Some(Language::Objectscript));
                } else {
                    assert_eq!(method.private_variables.len(), 0);
                    assert_eq!(method.is_procedure_block, Some(false));
                    assert_eq!(method.language, Some(Language::Objectscript));
                }
            }
        }
    }
}
