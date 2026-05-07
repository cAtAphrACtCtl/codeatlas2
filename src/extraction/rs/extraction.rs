use std::path::Path;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

use crate::repo::repo::{
    FileNode, FunctionInfo, ImportInfo, Span, StructInfo, SymbolInfo, SymbolNode, get_id,
};

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
                    resolved_path: None,
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
                resolved_path: None,
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
                resolved_path: None,
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
                    resolved_path: None,
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
                        resolved_path: None,
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

fn canonical_module_path(path: &str) -> String {
    if path.is_empty() {
        String::from("crate")
    } else if path == "crate" || path.starts_with("crate::") {
        path.to_string()
    } else {
        format!("crate::{}", path)
    }
}

fn make_qualified_name(module_path: &str, name: &str) -> String {
    let module = canonical_module_path(module_path);
    if name.is_empty() {
        module.to_string()
    } else {
        format!("{}::{}", module, name)
    }
}

fn extract_struct_members(node: &Node, source: &[u8]) -> Vec<String> {
    let Some(body) = node
        .child_by_field_name("body")
        .or_else(|| find_named_child(node, "field_declaration_list"))
        .or_else(|| find_named_child(node, "ordered_field_declaration_list"))
    else {
        return vec![];
    };

    if body.kind() == "ordered_field_declaration_list" {
        let mut cursor = body.walk();
        return body
            .named_children(&mut cursor)
            .filter(|child| !matches!(child.kind(), "attribute_item" | "visibility_modifier"))
            .filter_map(|child| child.utf8_text(source).ok())
            .map(String::from)
            .collect();
    }

    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter_map(|child| extract_struct_member(&child, source))
        .collect()
}

fn extract_struct_member(node: &Node, source: &[u8]) -> Option<String> {
    if node.kind() != "field_declaration" {
        return None;
    }

    node.child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .or_else(|| {
            node.child_by_field_name("type")
                .and_then(|field_type| field_type.utf8_text(source).ok())
        })
        .map(String::from)
}

fn resolve_import_path(current_module: &str, raw_import_path: &str) -> String {
    if raw_import_path.starts_with("crate::") {
        return raw_import_path.to_string();
    }

    if raw_import_path.starts_with("self::") {
        return format!(
            "{}::{}",
            canonical_module_path(current_module),
            raw_import_path.trim_start_matches("self::")
        );
    }

    if raw_import_path.starts_with("super::") {
        let mut remainder = raw_import_path;
        let mut super_count = 0usize;
        while let Some(next) = remainder.strip_prefix("super::") {
            super_count += 1;
            remainder = next;
        }

        let mut parts = canonical_module_path(current_module)
            .split("::")
            .map(String::from)
            .collect::<Vec<_>>();

        // Keep at least the crate root when super:: goes past current depth.
        let max_pops = parts.len().saturating_sub(1);
        let pop_count = super_count.min(max_pops);
        for _ in 0..pop_count {
            parts.pop();
        }

        if remainder.is_empty() {
            return parts.join("::");
        }

        return format!("{}::{}", parts.join("::"), remainder);
    }

    let first = raw_import_path.split("::").next().unwrap_or("");
    if matches!(first, "std" | "core" | "alloc") {
        raw_import_path.to_string()
    } else {
        format!("crate::{}", raw_import_path)
    }
}

/// Walk up the ancestor chain and collect any enclosing `mod_item` names in
/// order from outermost to innermost, then append them to `file_module_path`.
fn module_path_for_node(node: &Node, source: &[u8], file_module_path: &str) -> String {
    let mut mod_names: Vec<String> = Vec::new();
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "mod_item" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(source) {
                    mod_names.push(name.to_string());
                }
            }
        }
        current = parent.parent();
    }
    mod_names.reverse();
    if mod_names.is_empty() {
        file_module_path.to_string()
    } else {
        format!("{}::{}", file_module_path, mod_names.join("::"))
    }
}

fn query_symbols<F>(query: &str, tree: &Tree, source: &str, mut builder: F) -> Vec<SymbolNode>
where
    F: FnMut(&Node) -> Vec<SymbolNode>,
{
    let query = match Query::new(&tree_sitter_rust::LANGUAGE.into(), query) {
        Ok(q) => q,
        Err(_) => {
            eprintln!("Unable to parse query: {}", query);
            return vec![];
        }
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut symbols = Vec::new();
    while let Some(m) = matches.next() {
        for c in m.captures.iter() {
            symbols.extend(builder(&c.node));
        }
    }

    symbols
}

pub(crate) fn rs_extract_imports(
    tree: &Tree,
    source: &str,
    file_node: &FileNode,
    module_path: &str,
) -> Vec<SymbolNode> {
    let file_module_path = canonical_module_path(module_path);
    let source_bytes = source.as_bytes();

    query_symbols(
        r#"(use_declaration) @import"#,
        tree,
        source,
        |node: &Node| {
            let effective_module = module_path_for_node(node, source_bytes, &file_module_path);
            extract_import_info(node, source_bytes)
                .into_iter()
                .map(|mut import| {
                    import.resolved_path =
                        Some(resolve_import_path(&effective_module, &import.import_path));
                    let name = import
                        .alias
                        .clone()
                        .unwrap_or_else(|| import.import_path.clone());
                    SymbolNode {
                        id: get_id(),
                        file: file_node.id,
                        name: name.clone(),
                        qualified_name: make_qualified_name(&effective_module, &name),
                        module_path: Some(effective_module.clone()),
                        info: crate::repo::repo::SymbolInfo::Import(import),
                        span: extract_span(node),
                    }
                })
                .collect()
        },
    )
}

pub(crate) fn rs_extract_functions(
    tree: &Tree,
    source: &str,
    file_node: &FileNode,
    module_path: &str,
) -> Vec<SymbolNode> {
    let file_module_path = canonical_module_path(module_path);
    let source_bytes = source.as_bytes();

    query_symbols(
        r#"(function_item) @function"#,
        tree,
        source,
        |node: &Node| {
            let effective_module = module_path_for_node(node, source_bytes, &file_module_path);
            let args = node.child_by_field_name("parameters").map(|params| {
                let mut cursor = params.walk();
                params
                    .named_children(&mut cursor)
                    .filter(|n| matches!(n.kind(), "parameter" | "self_parameter"))
                    .filter_map(|n| n.utf8_text(source_bytes).ok())
                    .map(|text| text.to_string())
                    .collect::<Vec<_>>()
            });
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("unknown")
                .to_string();
            vec![SymbolNode {
                id: get_id(),
                file: file_node.id,
                name: String::from(&name),
                qualified_name: make_qualified_name(&effective_module, &name),
                module_path: Some(effective_module),
                info: crate::repo::repo::SymbolInfo::Function(FunctionInfo { args }),
                span: extract_span(node),
            }]
        },
    )
}

pub(crate) fn rs_extract_structs(
    tree: &Tree,
    source: &str,
    file_node: &FileNode,
    module_path: &str,
) -> Vec<SymbolNode> {
    let file_module_path = canonical_module_path(module_path);
    let source_bytes = source.as_bytes();
    query_symbols(r#"(struct_item) @struct"#, tree, source, |node: &Node| {
        let effective_module = module_path_for_node(node, source_bytes, &file_module_path);
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("unknown")
            .to_string();
        let members = extract_struct_members(node, source_bytes);

        vec![SymbolNode {
            id: get_id(),
            file: file_node.id,
            name: name.clone(),
            qualified_name: make_qualified_name(&effective_module, &name),
            module_path: Some(effective_module),
            info: SymbolInfo::Struct(StructInfo { members }),
            span: extract_span(node),
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_module_path_for_file_handles_root_and_mod_files() {
        assert_eq!(module_path_for_file(&PathBuf::from("src/lib.rs")), "crate");
        assert_eq!(
            module_path_for_file(&PathBuf::from("src/repo/mod.rs")),
            "crate::repo"
        );
        assert_eq!(
            module_path_for_file(&PathBuf::from("src/extraction/rs/extraction.rs")),
            "crate::extraction::rs::extraction"
        );
    }

    #[test]
    fn test_make_qualified_name_uses_canonical_module_path() {
        assert_eq!(make_qualified_name("repo", "add"), "crate::repo::add");
        assert_eq!(
            make_qualified_name("crate::repo", "add"),
            "crate::repo::add"
        );
    }

    #[test]
    fn test_resolve_import_path_handles_relative_and_std_paths() {
        assert_eq!(
            resolve_import_path("crate::repo::sub", "self::inner::Thing"),
            "crate::repo::sub::inner::Thing"
        );
        assert_eq!(
            resolve_import_path("crate::repo::sub", "super::util::helper"),
            "crate::repo::util::helper"
        );
        assert_eq!(
            resolve_import_path("crate::repo::sub::leaf", "super::super::util::helper"),
            "crate::repo::util::helper"
        );
        assert_eq!(
            resolve_import_path("crate::repo::sub", "std::fs"),
            "std::fs"
        );
    }

    fn make_file_node() -> FileNode {
        use crate::repo::repo::{FileId, LanguageKind};
        FileNode {
            id: FileId(1),
            path: std::path::PathBuf::from("src/repo/repo.rs"),
            language: LanguageKind::Rust,
        }
    }

    fn get_rust_parser() -> tree_sitter::Parser {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");
        parser
    }

    #[test]
    fn test_rs_extract_functions_nested_mod_sets_correct_module_path() {
        let source = r#"
mod inner {
    pub fn helper() {}
    mod deep {
        pub fn nested() {}
    }
}
fn top_level() {}
"#;
        let tree = get_rust_parser().parse(source, None).expect("parse");
        let file_node = make_file_node();

        let symbols = rs_extract_functions(&tree, source, &file_node, "crate::repo::repo");

        let by_name = |name: &str| {
            symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("symbol '{}' not found", name))
        };

        let helper = by_name("helper");
        assert_eq!(
            helper.module_path.as_deref(),
            Some("crate::repo::repo::inner")
        );
        assert_eq!(helper.qualified_name, "crate::repo::repo::inner::helper");

        let nested = by_name("nested");
        assert_eq!(
            nested.module_path.as_deref(),
            Some("crate::repo::repo::inner::deep")
        );
        assert_eq!(
            nested.qualified_name,
            "crate::repo::repo::inner::deep::nested"
        );

        let top = by_name("top_level");
        assert_eq!(top.module_path.as_deref(), Some("crate::repo::repo"));
        assert_eq!(top.qualified_name, "crate::repo::repo::top_level");
    }

    #[test]
    fn test_rs_extract_imports_nested_mod_sets_correct_module_path() {
        let source = r#"
mod utils {
    use std::collections::HashMap;
}
use std::fs;
"#;
        let tree = get_rust_parser().parse(source, None).expect("parse");
        let file_node = make_file_node();

        let symbols = rs_extract_imports(&tree, source, &file_node, "crate::repo::repo");

        let hashmap_sym = symbols
            .iter()
            .find(|s| s.name.contains("HashMap"))
            .expect("HashMap import");
        assert_eq!(
            hashmap_sym.module_path.as_deref(),
            Some("crate::repo::repo::utils")
        );

        let fs_sym = symbols
            .iter()
            .find(|s| s.name.contains("fs"))
            .expect("fs import");
        assert_eq!(fs_sym.module_path.as_deref(), Some("crate::repo::repo"));
    }

    #[test]
    fn test_rs_extract_structs_collects_members_and_nested_mod_paths() {
        let source = r#"
pub struct Config {
    enabled: bool,
    pub count: usize,
}

mod inner {
    struct State {
        name: String,
        active: bool,
    }
}

struct Unit;
"#;
        let tree = get_rust_parser().parse(source, None).expect("parse");
        let file_node = make_file_node();

        let symbols = rs_extract_structs(&tree, source, &file_node, "crate::repo::repo");

        let by_name = |name: &str| {
            symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("symbol '{}' not found", name))
        };

        let config = by_name("Config");
        assert_eq!(config.module_path.as_deref(), Some("crate::repo::repo"));
        assert_eq!(config.qualified_name, "crate::repo::repo::Config");
        assert!(matches!(
            &config.info,
            SymbolInfo::Struct(StructInfo { members }) if members == &vec![String::from("enabled"), String::from("count")]
        ));

        let state = by_name("State");
        assert_eq!(
            state.module_path.as_deref(),
            Some("crate::repo::repo::inner")
        );
        assert_eq!(state.qualified_name, "crate::repo::repo::inner::State");
        assert!(matches!(
            &state.info,
            SymbolInfo::Struct(StructInfo { members }) if members == &vec![String::from("name"), String::from("active")]
        ));

        let unit = by_name("Unit");
        assert!(matches!(
            &unit.info,
            SymbolInfo::Struct(StructInfo { members }) if members.is_empty()
        ));
    }

    #[test]
    fn test_rs_extract_structs_collects_tuple_struct_field_types() {
        let source = r#"
pub struct EdgeId(u64);
struct Pair(pub String, bool);
"#;
        let tree = get_rust_parser().parse(source, None).expect("parse");
        let file_node = make_file_node();

        let symbols = rs_extract_structs(&tree, source, &file_node, "crate::repo::repo");

        let by_name = |name: &str| {
            symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("symbol '{}' not found", name))
        };

        let edge_id = by_name("EdgeId");
        assert!(matches!(
            &edge_id.info,
            SymbolInfo::Struct(StructInfo { members }) if members == &vec![String::from("u64")]
        ));

        let pair = by_name("Pair");
        assert!(matches!(
            &pair.info,
            SymbolInfo::Struct(StructInfo { members }) if members == &vec![String::from("String"), String::from("bool")]
        ));
    }
}
