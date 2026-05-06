use crate::repo::repo::{OUTPUT_PATH, Repo, Span, SymbolInfo, SymbolNode};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub fn find_commands() -> Command {
    Command::new("find")
        .about("find symbol information")
        .arg(Arg::new("symbol").help("symbol to query").required(true))
        .arg(
            Arg::new("list")
                .help("list members")
                .action(ArgAction::SetTrue)
                .short('l'),
        )
        .arg(Arg::new("dir").help("search in directory").short('d'))
        .arg(Arg::new("repo").help("search in repo").short('r'))
        .group(
            ArgGroup::new("search_scope")
                .args(["dir", "repo"])
                .multiple(false),
        )
}

pub fn find_handle_command(matches: &ArgMatches) {
    let query = find_parse_command(matches);
    match find(query) {
        Ok(result) => println!("{}", result),
        Err(error) => eprintln!("failed to find symbol: {}", error),
    }
}

fn find_parse_command(matches: &ArgMatches) -> FindQuery {
    let symbol = matches
        .get_one::<String>("symbol")
        .expect("symbol is required");

    let scope = match (
        matches.get_one::<String>("dir"),
        matches.get_one::<String>("repo"),
    ) {
        (Some(dir), None) => FindScope::Dir(dir.to_string()),
        (None, Some(repo)) => FindScope::Repo(repo.to_string()),
        (_, _) => FindScope::All,
    };

    let mode = matches.get_flag("list");

    FindQuery {
        symbol: symbol.to_string(),
        scope: scope,
        list_mode: mode,
    }
}

#[derive(Debug, Eq, PartialEq)]
enum FindScope {
    Dir(String),
    Repo(String),
    All,
}

#[derive(Debug, Eq, PartialEq)]
pub struct FindQuery {
    symbol: String,
    scope: FindScope,
    list_mode: bool,
}

#[derive(Debug, serde::Serialize)]
struct SymbolSearchResult {
    name: String,
    qualified_name: String,
    kind: &'static str,
    path: String,
    module_path: Option<String>,
    span: Span,
    info: SymbolInfo,
}

impl SymbolSearchResult {
    fn new(path: PathBuf, symbol: SymbolNode) -> Self {
        let kind = symbol_kind(&symbol.info);

        SymbolSearchResult {
            name: symbol.name,
            qualified_name: symbol.qualified_name,
            kind,
            path: path.to_string_lossy().into_owned(),
            module_path: symbol.module_path,
            span: symbol.span,
            info: symbol.info,
        }
    }
}

fn symbol_kind(info: &SymbolInfo) -> &'static str {
    match info {
        SymbolInfo::Function(_) => "function",
        SymbolInfo::Struct(_) => "struct",
        SymbolInfo::Import(_) => "import",
    }
}

pub fn find(query: FindQuery) -> io::Result<String> {
    let dir_scope = match &query.scope {
        FindScope::Dir(path) => Some(PathBuf::from(path).canonicalize()?),
        _ => None,
    };
    let repos = load_repos(&query.scope)?;

    Ok(find_in_repos(&query, &repos, dir_scope.as_deref()))
}

fn load_repos(scope: &FindScope) -> io::Result<Vec<Repo>> {
    match scope {
        FindScope::Repo(repo_name) => {
            load_repo(Path::new(OUTPUT_PATH).join(repo_name)).map(|repo| vec![repo])
        }
        FindScope::Dir(_) | FindScope::All => load_all_repos(),
    }
}

fn load_all_repos() -> io::Result<Vec<Repo>> {
    let output_dir = Path::new(OUTPUT_PATH);
    if !output_dir.exists() {
        return Ok(vec![]);
    }

    let mut repos = Vec::new();
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            repos.push(load_repo(entry.path())?);
        }
    }

    Ok(repos)
}

fn load_repo(path: PathBuf) -> io::Result<Repo> {
    let file = File::open(path)?;
    serde_json::from_reader(file).map_err(io::Error::other)
}

fn find_in_repos(query: &FindQuery, repos: &[Repo], dir_scope: Option<&Path>) -> String {
    let mut matches = repos
        .iter()
        .flat_map(|repo| {
            repo.find_symbols(&query.symbol)
                .into_iter()
                .filter_map(|symbol| {
                    let absolute_file_path = repo.resolve_symbol_file_path(symbol.id)?;
                    Some((absolute_file_path, symbol))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    if let Some(dir_scope) = dir_scope {
        matches.retain(|(absolute_file_path, _)| absolute_file_path.starts_with(dir_scope));
    }

    matches.sort_by(|left, right| {
        (
            &left.0,
            &left.1.qualified_name,
            left.1.span.start_line,
            left.1.span.start_col,
        )
            .cmp(&(
                &right.0,
                &right.1.qualified_name,
                right.1.span.start_line,
                right.1.span.start_col,
            ))
    });

    let _ = query.list_mode;
    let symbols = matches
        .into_iter()
        .map(|(path, symbol)| SymbolSearchResult::new(path, symbol))
        .collect::<Vec<_>>();

    render_matches(&symbols)
}

fn render_matches(matches: &[SymbolSearchResult]) -> String {
    serde_json::to_string_pretty(matches).expect("serialize symbol matches")
}

#[cfg(test)]
mod tests {
    use crate::repo::repo::Repo;
    use crate::search::find::{
        FindQuery, FindScope, find_commands, find_in_repos, find_parse_command,
    };
    use std::path::Path;

    fn parse(args: Vec<&str>) -> FindQuery {
        let matches = find_commands()
            .try_get_matches_from(args.as_slice())
            .expect("failed to parse args");

        find_parse_command(&matches)
    }

    fn sample_repo() -> Repo {
        let root_path = std::env::current_dir()
            .expect("current directory")
            .join("repo-root");

        serde_json::from_value(serde_json::json!({
            "id": 1,
            "name": "codeatlas",
            "root_path": root_path,
            "files": [
                {
                    "id": 2,
                    "path": "src/repo.rs",
                    "language": "Rust"
                }
            ],
            "symbols": [
                {
                    "id": 3,
                    "file": 999,
                    "name": "EdgeId",
                    "qualified_name": "crate::repo::EdgeId",
                    "module_path": "crate::repo",
                    "info": {
                        "Struct": {
                            "members": ["u64"]
                        }
                    },
                    "span": {
                        "start_line": 27,
                        "start_col": 1,
                        "end_line": 27,
                        "end_col": 24
                    }
                }
            ],
            "edges": [
                {
                    "id": 4,
                    "kind": "Contain",
                    "from": {
                        "Repo": 1
                    },
                    "to": {
                        "File": 2
                    }
                },
                {
                    "id": 5,
                    "kind": "Contain",
                    "from": {
                        "File": 2
                    },
                    "to": {
                        "Symbol": 3
                    }
                }
            ]
        }))
        .expect("parse sample repo")
    }

    #[test]
    fn test_find_parse_symbol_only() {
        let expected_query = FindQuery {
            symbol: String::from("a"),
            scope: FindScope::All,
            list_mode: false,
        };

        let query = parse(vec!["find", "a"]);
        assert_eq!(expected_query, query);
    }

    #[test]
    fn test_find_parse_symbol_and_dir() {
        let expected_query = FindQuery {
            symbol: String::from("a"),
            scope: FindScope::Dir(String::from("path")),
            list_mode: false,
        };

        let query = parse(vec!["find", "a", "-d", "path"]);
        assert_eq!(expected_query, query);
    }

    #[test]
    fn test_find_parse_symbol_with_dir_and_list() {
        let expected_query = FindQuery {
            symbol: String::from("a"),
            scope: FindScope::Repo(String::from("repo")),
            list_mode: true,
        };

        let query = parse(vec!["find", "a", "-r", "repo", "-l"]);
        assert_eq!(expected_query, query);
    }

    #[test]
    fn test_find_in_repos_returns_where_and_symbol_type() {
        let output = find_in_repos(
            &FindQuery {
                symbol: String::from("EdgeId"),
                scope: FindScope::All,
                list_mode: false,
            },
            &[sample_repo()],
            None,
        );

        let symbols =
            serde_json::from_str::<Vec<serde_json::Value>>(&output).expect("parse symbol json");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0]["name"], "EdgeId");
        assert_eq!(symbols[0]["qualified_name"], "crate::repo::EdgeId");
        assert_eq!(symbols[0]["kind"], "struct");
        assert_eq!(symbols[0]["module_path"], "crate::repo");
        assert_eq!(symbols[0]["span"]["start_line"], 27);
        let path = symbols[0]["path"]
            .as_str()
            .expect("path should be a string");
        assert!(
            Path::new(path).is_absolute(),
            "expected path to be absolute, got {path}"
        );
        assert!(
            path.replace('\\', "/").ends_with("repo-root/src/repo.rs"),
            "expected path to end with repo-root/src/repo.rs, got {path}"
        );
        assert_eq!(symbols[0]["info"]["Struct"]["members"][0], "u64");
    }

    #[test]
    fn test_find_in_repos_list_mode_includes_details() {
        let output = find_in_repos(
            &FindQuery {
                symbol: String::from("EdgeId"),
                scope: FindScope::All,
                list_mode: true,
            },
            &[sample_repo()],
            None,
        );

        let symbols =
            serde_json::from_str::<Vec<serde_json::Value>>(&output).expect("parse symbol json");

        assert_eq!(symbols[0]["info"]["Struct"]["members"][0], "u64");
    }
}
