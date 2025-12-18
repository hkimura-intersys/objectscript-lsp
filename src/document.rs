use crate::scope_tree::*;
use tree_sitter::{Tree, Range, Node};
use url::Url;
use crate::parse_structures::{Class, FileType};
use crate::semantic::{LocalSemanticModel, get_node_children};

#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) content: String, // TODO: Rope provides O(log n) for text edits, insertions, and deletions compared to String's O(n) operations; might wanna use it
    pub(crate) tree: Tree,
    version: Option<i32>, // None if it hasn't been synced yet
    pub(crate) file_type: FileType,
    pub(crate) scope_tree: Option<ScopeTree>,
    pub(crate) uri: Url,
    pub local_semantic_model: Option<LocalSemanticModel>,
}

impl Document {
    pub fn new(
        content: String,
        tree: Tree,
        file_type: FileType,
        uri: Url,
    ) -> Self {
        Self {
            content,
            tree,
            version: None,
            file_type,
            scope_tree: None,
            uri,
            local_semantic_model: None,
        }
    }

    pub fn initial_build_scope_tree(&mut self)  {
        let mut scope_tree = ScopeTree::new(self.content.to_string());
        let mut scope_stack = vec![scope_tree.root];
        walk_tree(self.tree.clone().root_node(), &mut |node| {
            if cls_is_scope_node(node) {
                let scope_id = scope_tree.add_scope(
                    node.start_position(),
                    node.end_position(),
                    *scope_stack.last().unwrap(),
                    None,
                    false
                );
                scope_stack.push(scope_id);
            }
        });
        self.scope_tree = Some(scope_tree);
    }

    /// Passes in the Root Node (Should be source_file), and uses that the build the scope tree,
    /// and then builds the class struct, and then builds the local semantic model. Adds global
    /// variables and public local variables to the global semantic model. Returns the
    /// class name.
    pub(crate) fn initial_build(&mut self, node: Node) -> Class {
        // first, build the scope tree
        self.initial_build_scope_tree();
        // gives us the named nodes (include/includegenerators/import, class_definition)
        let scope = self.scope_tree.clone().unwrap().find_current_scope(node.start_position()).unwrap();
        let mut import_values = Vec::new();
        let mut include_values = Vec::new();
        let mut include_generator_values = Vec::new();
        let class_def = node.named_child(node.named_child_count() - 1).unwrap();
        let class_name = self.content[class_def.named_child(1).unwrap().byte_range()].to_string();
        if node.named_child_count() > 1 {
            // this means there are some imports/includegen/include statements
            // got the class name node and then got that byte range
            let children = get_node_children(node.clone());
            for header_node in children[..node.named_child_count()-1].iter() {
                // looping through the imports/includegen/include statements
                // header_node is either import_code, include_generator, or include_code
                let include_clause =  header_node.named_child(1);
                match header_node.kind() {
                    "import_code" => {
                        for child in get_node_children(include_clause.unwrap()) {
                            import_values.push(self.content[child.byte_range()].to_string());
                        }
                    },
                    "include_code" => {
                        for child in get_node_children(include_clause.unwrap()) {
                            include_values.push(self.content[child.byte_range()].to_string());
                        }
                    },
                    "include_generator" => {
                        for child in get_node_children(include_clause.unwrap()) {
                            include_generator_values.push(self.content[child.byte_range()].to_string());
                        }
                    }
                    _ => println!("UNKNOWN NODE {:?}", header_node.kind())
                }
            }
        }
        let class = Class::new(class_name.clone(), scope, import_values, include_values, include_generator_values);
        let mut local_semantic = LocalSemanticModel::new(class, self.content.clone());
        println!("{:#?}", local_semantic);
        local_semantic.cls_build_symbol_table(class_def);
        self.local_semantic_model = Some(local_semantic);
        self.local_semantic_model.as_ref().unwrap().class.clone()
    }
}
