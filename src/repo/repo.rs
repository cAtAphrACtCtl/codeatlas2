use crate::extraction::rs::extraction::{module_path_for_file, rs_extract_functions, rs_extract_imports, rs_extract_structs};
use clap::{Arg, ArgMatches, Command};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tree_sitter::{Parser};
use walkdir::WalkDir;

pub(crate) static OUTPUT_PATH: &str = "output";
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd,
)]
pub(crate) struct RepoId(u64);
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd, Ord,
)]
pub struct FileId(pub(crate) u64);
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd,
)]
pub struct SymbolId(u64);
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd,
)]
pub struct EdgeId(u64);

pub(crate) trait  IdType {
    fn from_raw(id: u64) -> Self;
}
impl IdType for RepoId {
    fn from_raw(id: u64) -> RepoId {
        RepoId(id)
    }
}
impl IdType for FileId {
    fn from_raw(id: u64) -> FileId {
        FileId(id)
    }
}
impl IdType for SymbolId {
    fn from_raw(id: u64) -> SymbolId {
        SymbolId(id)
    }
}

impl IdType for EdgeId {
    fn from_raw(id: u64) -> EdgeId {
        EdgeId(id)
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn get_id<T: IdType>() -> T {
    T::from_raw(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug, serde::Deserialize, serde::Serialize, PartialOrd, PartialEq, Ord, Eq)]
pub(crate) struct FileNode {
    pub(crate) id: FileId,
    pub(crate) path: PathBuf,
    pub(crate) language: LanguageKind,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct Repo {
    id: RepoId,
    name: String,
    root_path: PathBuf,
    files: Vec<FileNode>,
    symbols: Vec<SymbolNode>,
    edges: Vec<Edge>,
}

impl Repo {
    pub(crate) fn new(name: String, path: PathBuf, files: Vec<FileNode>) -> Repo {
        Repo {
            id: get_id(),
            name,
            root_path: path,
            files,
            symbols: vec![],
            edges: vec![],
        }
    }

    fn repo_root(&self) -> &Path {
        repo_root(self.root_path.as_path())
    }

    fn resolve_file_path(&self, file: &Path) -> PathBuf {
        if file.is_absolute() {
            file.to_path_buf()
        } else {
            self.repo_root().join(file)
        }
    }

    fn extract_symbols(&self) -> Vec<SymbolNode> {
        let mut parser = get_parser();
        let mut symbols = Vec::new();
        for file in &self.files {
            let file_path = self.resolve_file_path(&file.path);
            let module_path = module_path_for_file(&file.path);

            let source = match fs::read_to_string(&file_path) {
                Ok(s) => s,
                Err(_) => {
                    eprintln!("Unable to read source file: {}", file_path.display());
                    continue;
                }
            };

            let tree = match parser.parse(&source, None) {
                Some(tree) => tree,
                None => {
                    eprintln!("Unable to parse source file: {}", file_path.display());
                    continue;
                }
            };
            symbols.append(rs_extract_imports(&tree, &source, &file, &module_path).as_mut());
            symbols.append(rs_extract_functions(&tree, &source, &file, &module_path).as_mut());
            symbols.append(rs_extract_structs(&tree, &source, &file, &module_path).as_mut());
        }

        symbols
    }

    fn build_edges(&self) -> Vec<Edge> {
        let mut edges = self
            .files
            .iter()
            .map(|file| Edge {
                id: get_id(),
                kind: EdgeKind::Contain,
                from: NodeRef::Repo(self.id),
                to: NodeRef::File(file.id),
            })
            .collect::<Vec<_>>();

        edges.extend(self.symbols.iter().map(|symbol| Edge {
            id: get_id(),
            kind: EdgeKind::Contain,
            from: NodeRef::File(symbol.file),
            to: NodeRef::Symbol(symbol.id),
        }));

        edges
    }

    fn resolve_symbol_file_id(&self, symbol_id: SymbolId) -> Option<FileId> {
        self.edges.iter().find_map(|edge| match (&edge.kind, &edge.from, &edge.to) {
            (EdgeKind::Contain, NodeRef::File(file_id), NodeRef::Symbol(candidate_symbol_id))
                if *candidate_symbol_id == symbol_id => Some(*file_id),
            _ => None,
        })
    }

    fn is_repo_file(&self, file_id: FileId) -> bool {
        self.edges.iter().any(|edge| {
            matches!(
                (&edge.kind, &edge.from, &edge.to),
                (EdgeKind::Contain, NodeRef::Repo(repo_id), NodeRef::File(candidate_file_id))
                    if *repo_id == self.id && *candidate_file_id == file_id
            )
        })
    }

    pub(crate) fn resolve_symbol_file_path(&self, symbol_id: SymbolId) -> Option<PathBuf> {
        let file_id = self.resolve_symbol_file_id(symbol_id)?;
        if !self.is_repo_file(file_id) {
            return None;
        }

        let file = self.files.iter().find(|file| file.id == file_id)?;
        Some(self.resolve_file_path(&file.path))
    }

    pub(crate) fn find_symbols(&self, query: &str) -> Vec<SymbolNode> {
        self.symbols
            .iter()
            .filter(|symbol| symbol.name == query || symbol.qualified_name == query)
            .filter(|symbol| self.resolve_symbol_file_path(symbol.id).is_some())
            .cloned()
            .collect()
    }

    pub(super) fn export_json(&self, output_file: &Path) -> std::io::Result<()> {
        if let Some(parent) = output_file.parent() {
            fs::create_dir_all(parent)?
        }

        let file = File::create(output_file)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &self).map_err(std::io::Error::other)?;
        writer.flush()?;

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) enum SymbolInfo {
    Function(FunctionInfo),
    Struct(StructInfo),
    Import(ImportInfo),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct FunctionInfo {
    pub(crate) args: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct StructInfo {
    pub(crate) members: Vec<String>,
}
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ImportInfo {
    pub(crate) import_path: String,
    pub(crate) alias: Option<String>,
    pub(crate) is_glob: bool,
    pub(crate) resolved_path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolNode {
    pub(crate) id: SymbolId,
    pub(crate) file: FileId,
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) module_path: Option<String>,
    pub(crate) info: SymbolInfo,
    pub(crate) span: Span,
}
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Span {
    pub(crate) start_line: usize,
    pub(crate) start_col: usize,
    pub(crate) end_line: usize,
    pub(crate) end_col: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Edge {
    id: EdgeId,
    kind: EdgeKind,
    from: NodeRef,
    to: NodeRef,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
enum NodeRef {
    Repo(RepoId),
    File(FileId),
    Symbol(SymbolId),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
enum EdgeKind {
    Contain,
    Define,
    Import,
    Call,
    Reference,
    Implement,
}
#[derive(Debug, serde::Deserialize, serde::Serialize, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum LanguageKind {
    Rust,
}

pub fn repo_commands() -> Command {
    Command::new("repo")
        .about("add or delete a repo")
        .subcommands([
            Command::new("add")
                .arg(
                    Arg::new("name")
                        .short('n')
                        .help("name to the repo")
                        .required(true),
                )
                .arg(
                    Arg::new("path")
                        .short('p')
                        .help("path to the repo")
                        .required(true),
                ),
            Command::new("del").arg(Arg::new("name").help("name to the repo").required(true)),
        ])
}

pub fn repo_handle_commands(matches: &ArgMatches) {
    match matches.subcommand() {
        Some(("add", sub_matches)) => {
            let query = AddQuery::parse(sub_matches);
            let mut repo = match repo_add(query) {
                Ok(repo) => repo,
                Err(e) => {
                    eprintln!("Failed to add repo {}", e);
                    return;
                }
            };
            repo.symbols = repo.extract_symbols();
            repo.edges = repo.build_edges();

            let output_file = Path::join(OUTPUT_PATH.as_ref(), repo.name.as_str());
            match repo.export_json(output_file.as_path()) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to export repo symbols: {}", e);
                }
            }
        }
        Some(("del", sub_matches)) => {
            let query = DeleteQuery::parse(sub_matches);
            repo_delete(query);
        }
        _ => unreachable!("unknown subcommand"),
    }
}

fn repo_add(mut query: AddQuery) -> std::io::Result<Repo> {
    query.path = query.path.canonicalize()?;
    let files = relativize_files(query.path.as_path(), walk_dir(query.path.as_path()));
    let files = files
        .into_iter()
        .map(|f| FileNode {
            id: get_id(),
            path: f,
            language: LanguageKind::Rust,
        })
        .collect::<Vec<_>>();
    Ok(Repo::new(query.repo, query.path, files))
}

fn repo_delete(_query: DeleteQuery) -> bool {
    false
}

fn walk_dir(path: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    if path.is_file() {
        files.push(PathBuf::from(path));
    } else {
        for entry in WalkDir::new(path) {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    if let Some(rs_file) = path.extension()
                        && rs_file == "rs"
                    {
                        files.push(entry.path().to_path_buf());
                    }
                } else if path.is_dir() {
                    continue;
                }
            } else {
                eprintln!("unable to read {:?}", path);
            }
        }
    }
    files
}

fn repo_root(path: &Path) -> &Path {
    if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    }
}

fn relativize_files(base_path: &Path, files: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = repo_root(base_path);

    files
        .into_iter()
        .map(|file| {
            file.strip_prefix(root)
                .unwrap_or(file.as_path())
                .to_path_buf()
        })
        .collect()
}

fn get_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to rust grammar");
    parser
}

use crate::query::query::CommandQuery;
#[derive(Debug, Eq, PartialEq)]
pub(super) struct AddQuery {
    repo: String,
    path: PathBuf,
}

impl CommandQuery for AddQuery {
    fn parse(matches: &ArgMatches) -> Self {
        let repo = matches
            .get_one::<String>("name")
            .expect("repo name is required");

        let path = matches
            .get_one::<String>("path")
            .expect("repo path is required");

        AddQuery {
            repo: repo.to_string(),
            path: PathBuf::from(path),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct DeleteQuery {
    repo: String,
}

impl CommandQuery for DeleteQuery {
    fn parse(matches: &ArgMatches) -> Self {
        let repo = matches
            .get_one::<String>("name")
            .expect("repo name is required");

        DeleteQuery {
            repo: repo.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use tree_sitter::{Query, QueryCursor, StreamingIterator};
    use crate::extraction::rs::extraction::extract_import_info;

    fn fixture_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
    }

    fn parse_use_imports(source: &str) -> Vec<ImportInfo> {
        let mut parser = get_parser();
        let tree = parser.parse(source, None).expect("parse source");
        let query = Query::new(
            &tree_sitter_rust::LANGUAGE.into(),
            r#"(use_declaration) @import"#,
        )
        .expect("parse query");
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let first = matches.next().expect("first use declaration");
        let node = first.captures[0].node;
        extract_import_info(&node, source.as_bytes())
    }

    #[test]
    fn test_repo_add() {
        let expected = AddQuery {
            repo: String::from("repo"),
            path: PathBuf::from("path"),
        };

        let binding =
            repo_commands().get_matches_from(vec!["repo", "add", "-n", "repo", "-p", "path"]);
        let matches = binding.subcommand_matches("add").expect("repo add");

        let query = AddQuery::parse(matches);
        assert_eq!(expected, query);
    }

    #[test]
    fn test_repo_delete() {
        let expected = DeleteQuery {
            repo: String::from("repo"),
        };

        let binding = repo_commands().get_matches_from(vec!["repo", "del", "repo"]);
        let matches = binding.subcommand_matches("del").expect("repo del");

        let query = DeleteQuery::parse(&matches);
        assert_eq!(expected, query);
    }

    #[test]
    fn test_repo_add_stores_directory_files_as_relative_paths() {
        let repo_path = fixture_path("unittest/repos/dir_case");

        let repo = repo_add(AddQuery {
            repo: String::from("repo"),
            path: repo_path.clone(),
        })
        .expect("repo add");

        let files = repo.files;
        let mut files = files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        files.sort();

        let mut expected = vec![
            PathBuf::from("src").join("lib.rs"),
            PathBuf::from("src").join("nested").join("mod.rs"),
        ];
        expected.sort();

        assert_eq!(
            repo_path.canonicalize().expect("canonical repo path"),
            repo.root_path
        );
        assert_eq!(expected, files);
    }

    #[test]
    fn test_repo_add_stores_single_file_as_relative_path() {
        let repo_path = fixture_path("unittest/repos/single_file/standalone.rs");

        let repo = repo_add(AddQuery {
            repo: String::from("repo"),
            path: repo_path.clone(),
        })
        .expect("repo add");

        assert_eq!(
            repo_path.canonicalize().expect("canonical file path"),
            repo.root_path
        );

        let files = repo.files;
        let mut files = files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        files.sort();
        assert_eq!(vec![PathBuf::from("standalone.rs")], files);
    }

    #[test]
    fn test_extract_symbols_reads_relative_repo_files() {
        let repo_path = fixture_path("src/repo").canonicalize().expect("repo dir");
        let file_node = FileNode {
            id: FileId(1),
            path: PathBuf::from("repo.rs"),
            language: LanguageKind::Rust,
        };
        let repo = Repo::new(String::from("repo"), repo_path, vec![file_node]);

        let symbols = repo.extract_symbols();

        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_build_edges_links_repo_files_and_symbols() {
        let file_id = FileId(11);
        let symbol_id = SymbolId(21);
        let mut repo = Repo::new(
            String::from("repo"),
            PathBuf::from("path"),
            vec![FileNode {
                id: file_id,
                path: PathBuf::from("repo.rs"),
                language: LanguageKind::Rust,
            }],
        );
        repo.symbols = vec![SymbolNode {
            id: symbol_id,
            file: file_id,
            name: String::from("EdgeId"),
            qualified_name: String::from("crate::repo::EdgeId"),
            module_path: Some(String::from("crate::repo")),
            info: SymbolInfo::Struct(StructInfo {
                members: vec![String::from("u64")],
            }),
            span: Span {
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 19,
            },
        }];

        let edges = repo.build_edges();

        assert_eq!(edges.len(), 2);
        assert!(edges.iter().any(|edge| {
            matches!(
                (&edge.kind, &edge.from, &edge.to),
                (EdgeKind::Contain, NodeRef::Repo(repo_id), NodeRef::File(id))
                    if *repo_id == repo.id && *id == file_id
            )
        }));
        assert!(edges.iter().any(|edge| {
            matches!(
                (&edge.kind, &edge.from, &edge.to),
                (EdgeKind::Contain, NodeRef::File(id), NodeRef::Symbol(symbol))
                    if *id == file_id && *symbol == symbol_id
            )
        }));
    }

    #[test]
    fn test_find_symbols_uses_edges_to_resolve_file_and_kind() {
        let file_id = FileId(11);
        let symbol_id = SymbolId(21);
        let mut repo = Repo::new(
            String::from("repo"),
            PathBuf::from("path"),
            vec![FileNode {
                id: file_id,
                path: PathBuf::from("src").join("repo.rs"),
                language: LanguageKind::Rust,
            }],
        );
        repo.symbols = vec![SymbolNode {
            id: symbol_id,
            file: FileId(999),
            name: String::from("EdgeId"),
            qualified_name: String::from("crate::repo::EdgeId"),
            module_path: Some(String::from("crate::repo")),
            info: SymbolInfo::Struct(StructInfo {
                members: vec![String::from("u64")],
            }),
            span: Span {
                start_line: 27,
                start_col: 1,
                end_line: 27,
                end_col: 24,
            },
        }];
        repo.edges = vec![
            Edge {
                id: EdgeId(31),
                kind: EdgeKind::Contain,
                from: NodeRef::Repo(repo.id),
                to: NodeRef::File(file_id),
            },
            Edge {
                id: EdgeId(32),
                kind: EdgeKind::Contain,
                from: NodeRef::File(file_id),
                to: NodeRef::Symbol(symbol_id),
            },
        ];

        let matches = repo.find_symbols("EdgeId");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "EdgeId");
        assert_eq!(matches[0].qualified_name, "crate::repo::EdgeId");
        assert_eq!(matches[0].module_path.as_deref(), Some("crate::repo"));
        assert_eq!(
            repo.resolve_symbol_file_path(matches[0].id),
            Some(PathBuf::from("path").join("src").join("repo.rs"))
        );

        match &matches[0].info {
            SymbolInfo::Struct(info) => {
                assert_eq!(info.members, vec![String::from("u64")]);
            }
            _ => panic!("expected struct symbol"),
        }
    }

    #[test]
    fn test_repo_new_assigns_unique_ids() {
        let repo1 = Repo::new(String::from("repo1"), PathBuf::from("path1"), vec![]);
        let repo2 = Repo::new(String::from("repo2"), PathBuf::from("path2"), vec![]);

        assert_ne!(repo1.id, repo2.id);
        assert!(repo1.id > RepoId(0));
        assert!(repo2.id > RepoId(0));
    }

    #[test]
    fn test_get_id_is_unique_across_threads() {
        let thread_count = 32;
        let barrier = Arc::new(Barrier::new(thread_count));

        let handles: Vec<_> = (0..thread_count)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    let i: RepoId = get_id();
                    i
                })
            })
            .collect();

        let ids: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("thread should finish"))
            .collect();

        let unique_ids: HashSet<_> = ids.iter().copied().collect();

        assert_eq!(thread_count, ids.len());
        assert_eq!(thread_count, unique_ids.len());
        assert!(ids.into_iter().all(|id| id > RepoId(0)));
    }

    #[test]
    fn test_extract_import_info_identifier() {
        let imports = parse_use_imports("use std::fs;\n");
        assert_eq!(
            imports,
            vec![ImportInfo {
                import_path: String::from("std::fs"),
                alias: None,
                is_glob: false,
                resolved_path: None,
            }]
        );
    }

    #[test]
    fn test_extract_import_info_use_as_clause() {
        let imports = parse_use_imports("use std::fs as file_system;\n");
        assert_eq!(
            imports,
            vec![ImportInfo {
                import_path: String::from("std::fs"),
                alias: Some(String::from("file_system")),
                is_glob: false,
                resolved_path: None,
            }]
        );
    }

    #[test]
    fn test_extract_import_info_use_wildcard() {
        let imports = parse_use_imports("use std::io::*;\n");
        assert_eq!(
            imports,
            vec![ImportInfo {
                import_path: String::from("std::io::*"),
                alias: None,
                is_glob: true,
                resolved_path: None,
            }]
        );
    }

    #[test]
    fn test_extract_import_info_scoped_use_list() {
        let imports = parse_use_imports("use std::io::{BufWriter, Write};\n");
        assert_eq!(
            imports,
            vec![
                ImportInfo {
                    import_path: String::from("std::io::BufWriter"),
                    alias: None,
                    is_glob: false,
                    resolved_path: None,
                },
                ImportInfo {
                    import_path: String::from("std::io::Write"),
                    alias: None,
                    is_glob: false,
                    resolved_path: None,
                }
            ]
        );
    }

    #[test]
    fn test_rs_extract_functions_sets_module_and_qualified_name() {
        let mut parser = get_parser();
        let source = "fn demo(a: i32) {}\n";
        let tree = parser.parse(source, None).expect("parse source");
        let file_node = FileNode {
            id: FileId(1),
            path: PathBuf::from("repo.rs"),
            language: LanguageKind::Rust,
        };

        let symbols = rs_extract_functions(&tree, source, &file_node, "repo");
        assert_eq!(symbols.len(), 1);

        let symbol = &symbols[0];
        assert_eq!(symbol.module_path.as_deref(), Some("crate::repo"));
        assert_eq!(symbol.qualified_name, "crate::repo::demo");
    }

    #[test]
    fn test_rs_extract_imports_sets_resolved_module_and_qualified_name() {
        let mut parser = get_parser();
        let source = "use self::inner::Thing as Alias;\n";
        let tree = parser.parse(source, None).expect("parse source");
        let file_node = FileNode {
            id: FileId(1),
            path: PathBuf::from("repo.rs"),
            language: LanguageKind::Rust,
        };

        let symbols = rs_extract_imports(&tree, source, &file_node, "crate::repo::sub");
        assert_eq!(symbols.len(), 1);

        let symbol = &symbols[0];
        assert_eq!(symbol.name, "Alias");
        assert_eq!(symbol.module_path.as_deref(), Some("crate::repo::sub"));
        assert_eq!(symbol.qualified_name, "crate::repo::sub::Alias");

        match &symbol.info {
            SymbolInfo::Import(import) => {
                assert_eq!(
                    import.resolved_path.as_deref(),
                    Some("crate::repo::sub::inner::Thing")
                );
            }
            _ => panic!("expected import symbol"),
        }
    }
}
