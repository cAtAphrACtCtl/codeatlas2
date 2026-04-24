use clap::{Arg, ArgMatches, Command};
use crate::extraction::rs::extraction::{extract_import_info, extract_span, module_path_for_file};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator, Tree};
use walkdir::WalkDir;

static OUTPUT_PATH: &str = "output";
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd,
)]
pub(crate) struct RepoId(u64);
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd, Ord,
)]
pub struct FileId(u64);
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd,
)]
pub struct SymbolId(u64);
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd,
)]
pub struct EdgeId(u64);

trait IdType {
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

fn get_id<T: IdType>() -> T {
    T::from_raw(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug, serde::Deserialize, serde::Serialize, PartialOrd, PartialEq, Ord, Eq)]
pub(crate) struct FileNode {
    id: FileId,
    path: PathBuf,
    language: LanguageKind,
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
        let language = &tree_sitter_rust::LANGUAGE.into();
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

            let source_bytes = source.as_bytes();
            let tree = match parser.parse(&source, None) {
                Some(tree) => tree,
                None => {
                    eprintln!("Unable to parse source file: {}", file_path.display());
                    continue;
                }
            };
            symbols.append(
                self.query_symbols(
                    language,
                    r#"(use_declaration) @import"#,
                    &tree,
                    &source,
                    |node: &Node| {
                        extract_import_info(node, source_bytes)
                            .into_iter()
                            .map(|import| {
                                let name = import
                                    .alias
                                    .clone()
                                    .unwrap_or_else(|| import.import_path.clone());
                                SymbolNode {
                                    id: get_id(),
                                    file: file.id,
                                    name,
                                    module_path: Some(module_path.clone()),
                                    info: SymbolInfo::Import(import),
                                    span: extract_span(node),
                                }
                            })
                            .collect()
                    },
                )
                .as_mut(),
            );
            symbols.append(
                self.query_symbols(
                    language,
                    r#"(function_item) @function"#,
                    &tree,
                    &source,
                    |node: &Node| {
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
                            file: file.id,
                            name: String::from(&name),
                            module_path: Some(module_path.clone()),
                            info: SymbolInfo::Function(FunctionInfo { args }),
                            span: extract_span(node),
                        }]
                    },
                )
                .as_mut(),
            );
        }

        symbols
    }

    fn query_symbols<F>(
        &self,
        language: &Language,
        query: &str,
        tree: &Tree,
        source: &str,
        mut builder: F,
    ) -> Vec<SymbolNode>
    where
        F: FnMut(&Node) -> Vec<SymbolNode>,
    {
        let query = match Query::new(language, query) {
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
enum SymbolInfo {
    Function(FunctionInfo),
    Struct(StructInfo),
    Import(ImportInfo),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct FunctionInfo {
    args: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct StructInfo {
    members: Vec<String>,
}
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ImportInfo {
    pub(crate) import_path: String,
    pub(crate) alias: Option<String>,
    pub(crate) is_glob: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SymbolNode {
    id: SymbolId,
    file: FileId,
    name: String,
    module_path: Option<String>,
    info: SymbolInfo,
    span: Span,
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
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
enum LanguageKind {
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
                },
                ImportInfo {
                    import_path: String::from("std::io::Write"),
                    alias: None,
                    is_glob: false,
                }
            ]
        );
    }
}
