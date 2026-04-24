use std::path::Path;
use tree_sitter::Node;

use crate::repo::repo::{ImportInfo, Span};

pub(crate) fn extract_span(node: &Node) -> Span {
    let start = node.start_position();
    let end = node.end_position();
    Span {
        start_line: start.row + 1,
        start_col: start.column + 1,
        end_line: end.row + 1,
        end_col: end.column + 1,
    }
}

pub(crate) fn extract_import_info(node: &Node, source: &[u8]) -> Vec<ImportInfo> {
    let arg = match node
        .child_by_field_name("argument")
        .or_else(|| node.named_child(0))
    {
        Some(arg) => arg,
        None => return vec![],
    };

    let mut result = vec![];
    collect_imports(&arg, source, "", &mut result);
    result
}

fn collect_imports(node: &Node, source: &[u8], prefix: &str, out: &mut Vec<ImportInfo>) {
    match node.kind() {
        "identifier" | "scoped_identifier" | "crate" | "self" | "super" => {
            if let Ok(text) = node.utf8_text(source) {
                out.push(ImportInfo {
                    import_path: join_path(prefix, text),
                    alias: None,
                    is_glob: false,
                });
            }
        }
        "use_as_clause" => {
            let path = node
                .child_by_field_name("path")
                .or_else(|| node.named_child(0))
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            let alias = node
                .child_by_field_name("alias")
                .or_else(|| node.named_child(1))
                .and_then(|n| n.utf8_text(source).ok())
                .map(String::from);

            out.push(ImportInfo {
                import_path: join_path(prefix, path),
                alias,
                is_glob: false,
            });
        }
        "use_wildcard" => {
            let path = node
                .child_by_field_name("path")
                .or_else(|| node.named_child(0))
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            let base = join_path(prefix, path);
            let import_path = if base.is_empty() {
                String::from("*")
            } else {
                format!("{}::*", base)
            };

            out.push(ImportInfo {
                import_path,
                alias: None,
                is_glob: true,
            });
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_imports(&child, source, prefix, out);
            }
        }
        "scoped_use_list" => {
            let path = node
                .child_by_field_name("path")
                .or_else(|| node.named_child(0))
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            let scoped_prefix = join_path(prefix, path);

            if let Some(list_node) = node
                .child_by_field_name("list")
                .or_else(|| find_named_child(node, "use_list"))
            {
                let mut cursor = list_node.walk();
                for child in list_node.named_children(&mut cursor) {
                    collect_imports(&child, source, &scoped_prefix, out);
                }
            }
        }
        "metavariable" => {
            if let Ok(text) = node.utf8_text(source) {
                out.push(ImportInfo {
                    import_path: join_path(prefix, text),
                    alias: None,
                    is_glob: false,
                });
            }
        }
        _ => {
            let mut cursor = node.walk();
            let children = node.named_children(&mut cursor).collect::<Vec<_>>();
            if children.is_empty() {
                if let Ok(text) = node.utf8_text(source) {
                    out.push(ImportInfo {
                        import_path: join_path(prefix, text),
                        alias: None,
                        is_glob: false,
                    });
                }
            } else {
                for child in children {
                    collect_imports(&child, source, prefix, out);
                }
            }
        }
    }
}

fn find_named_child<'a>(node: &'a Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn join_path(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else if path.is_empty() {
        prefix.to_string()
    } else {
        format!("{}::{}", prefix, path)
    }
}

pub(crate) fn module_path_for_file(path: &Path) -> String {
    let mut parts = path
        .with_extension("")
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .map(String::from)
        .collect::<Vec<_>>();

    if parts.first().map(String::as_str) == Some("src") {
        parts.remove(0);
    }

    if let Some(last) = parts.last() {
        if last == "mod" || last == "lib" || last == "main" {
            parts.pop();
        }
    }

    if parts.is_empty() {
        String::from("crate")
    } else {
        format!("crate::{}", parts.join("::"))
    }
}

