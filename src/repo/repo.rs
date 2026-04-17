use async_walkdir::{Filtering, WalkDir};
use clap::{Arg, ArgMatches, Command};
use futures_lite::StreamExt;
use futures_lite::future::block_on;
use std::path::Path;

static OUTPUT_PATH: &str = "output";

#[derive(Debug)]
pub struct Repo {
    name: String,
    path: String,
    files: Vec<String>,
}

impl Repo {
    pub fn new(name: String, path: String, files: Vec<String>) -> Repo {
        Repo { name, path, files }
    }
}

#[derive(Debug)]
pub struct Symbol {
    language: String,
    name: String,
    kind: String,
    location: SymbolLocation,
}
#[derive(Debug)]
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
            repo_add(query);
        }
        Some(("del", sub_matches)) => {
            let query = DeleteQuery::parse(sub_matches);
            repo_delete(query);
        }
        _ => unreachable!("unknown subcommand"),
    }
}

fn repo_add(query: AddQuery) -> bool {
    false
}

fn repo_delete(query: DeleteQuery) -> bool {
    false
}

fn walk_dir(path_str: &str) -> Vec<String> {
    let mut files = Vec::new();
    let path = Path::new(path_str);
    if path.is_file() {
        files.push(path_str.to_string());
    } else {
        block_on(async {
            let mut entries = WalkDir::new(path).filter(|p| async move {
                if let Some(true) = p
                    .path()
                    .file_name()
                    .map(|f| f.to_string_lossy().ends_with(".rs"))
                {
                    Filtering::Continue
                } else {
                    Filtering::Ignore
                }
            });
            loop {
                match entries.next().await {
                    Some(Ok(entry)) => {
                        if let Some(file_name) = entry.path().to_str() {
                            files.push(file_name.to_string());
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("error when walking repo path:{e}");
                        eprintln!("error:{e}");
                    }
                    None => break,
                }
            }
        });
    }
    files
}

use crate::query::query::Query;
#[derive(Debug, Eq, PartialEq)]
struct AddQuery {
    repo: String,
    path: String,
}

impl Query for AddQuery {
    fn parse(matches: &ArgMatches) -> Self {
        let repo = matches
            .get_one::<String>("name")
            .expect("repo name is required");

        let path = matches
            .get_one::<String>("path")
            .expect("repo path is required");

        AddQuery {
            repo: repo.to_string(),
            path: path.to_string(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct DeleteQuery {
    repo: String,
}

impl Query for DeleteQuery {
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
            path: String::from("path"),
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
