use clap::{Arg, ArgMatches, Command};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write, read_to_string};
use std::path::{Path, PathBuf};
use std::process::Output;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator, Tree};
use walkdir::WalkDir;

static OUTPUT_PATH: &str = "output";

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct Repo {
    name: String,
    path: PathBuf,
    files: Vec<PathBuf>,
    symbols: Vec<Symbol>,
}

impl Repo {
    pub(crate) fn new(name: String, path: PathBuf, files: Vec<PathBuf>) -> Repo {
        Repo {
            name,
            path,
            files,
            symbols: vec![],
        }
    }

    fn extract_symbols(&self) -> Vec<Symbol> {
        let mut parser = get_parser();
        let mut symbols = Vec::new();
        let language = &tree_sitter_rust::LANGUAGE.into();
        for file in &self.files {
            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => {
                    eprintln!("Unable to read source file: {}", file.display());
                    continue;
                }
            };

            let source_bytes = source.as_bytes();
            let tree = parser.parse(&source, None).unwrap();
            symbols.append(
                self.query_symbols(
                    language,
                    r#"(use_declaration) @import"#,
                    &tree,
                    &source,
                    |node: &Node| {
                        let text = node.utf8_text(source_bytes).unwrap();
                        let pos = node.start_position();
                        Symbol {
                            language: node
                                .language()
                                .name()
                                .unwrap_or_else(|| "unknow because the parser is old")
                                .to_string(),
                            info: SymbolInfo::Import(String::from(text)),
                            location: SymbolLocation {
                                file: file.display().to_string(),
                                line: pos.row + 1,
                                col: pos.column + 1,
                            },
                        }
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
                    |node: &tree_sitter::Node /* Type */| {
                        let pos = node.start_position();
                        let args = node.child_by_field_name("parameters").map(|params| {
                            let mut cursor = params.walk();
                            params
                                .named_children(&mut cursor)
                                .filter(|n| matches!(n.kind(), "parameter" | "self_parameter"))
                                .filter_map(|n| n.utf8_text(source_bytes).ok())
                                .map(|text| text.to_string())
                                .collect::<Vec<_>>()
                        });

                        Symbol {
                            language: node
                                .language()
                                .name()
                                .unwrap_or_else(|| "unknow because the parser is old")
                                .to_string(),
                            info: SymbolInfo::Function(FunctionInfo {
                                name: String::from(
                                    node.child_by_field_name("name")
                                        .and_then(|n| n.utf8_text(source_bytes).ok())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                ),
                                args,
                            }),
                            location: SymbolLocation {
                                file: file.display().to_string(),
                                line: pos.row + 1,
                                col: pos.column + 1,
                            },
                        }
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
    ) -> Vec<Symbol>
    where
        F: FnMut(&tree_sitter::Node) -> Symbol,
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
                symbols.push(builder(&c.node));
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
        serde_json::to_writer_pretty(&mut writer, &self).map_err((std::io::Error::other))?;
        writer.flush()?;

        Ok(())
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
enum SymbolInfo {
    Function(FunctionInfo),
    Struct(StructInfo),
    Import(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct FunctionInfo {
    name: String,
    args: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct StructInfo {
    name: String,
    members: Vec<String>,
    functions: Option<FunctionInfo>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Symbol {
    language: String,
    info: SymbolInfo,
    location: SymbolLocation,
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SymbolLocation {
    file: String,
    line: usize,
    col: usize,
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
            let mut repo= match repo_add(query) {
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
    if query.path.is_relative() {
        query.path = Path::new(&query.path)
            .canonicalize()?
    }
    let files = walk_dir(query.path.as_path());
    Ok(Repo::new(query.repo, query.path, files))
}

fn repo_delete(query: DeleteQuery) -> bool {
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
}
