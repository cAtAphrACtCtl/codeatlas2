use clap::{Arg, ArgMatches, Command};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

fn repo_add(mut query: AddQuery) -> bool {
    if query.path.is_relative() {
        query.path = Path::new(&query.path)
            .canonicalize()
            .expect("the relative path provided does not exist");
    }

    let files = walk_dir(query.path.as_path());

    println!("repo name: {}", query.repo);
    for file in files {
        println!("{}", file.display());
    }
    false
}

fn repo_delete(query: DeleteQuery) -> bool {
    false
}

fn walk_dir(path: &Path) -> Vec<PathBuf> {
    let mut files:Vec<PathBuf> = Vec::new();
    if path.is_file() {
        files.push(PathBuf::from(path));
    } else {
        for entry in WalkDir::new(path) {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    if let Some(rs_file) = path.extension() && rs_file == "rs" {
                        files.push(entry.path().to_owned());
                    }

                }else if path.is_dir() {
                    continue;
                }
            }else {
                println!("unable to read {:?}", path);
            }

        }
    }
    files
}

use crate::query::query::Query;
#[derive(Debug, Eq, PartialEq)]
pub(super) struct AddQuery {
    repo: String,
    path: PathBuf,
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
            path: PathBuf::from(path),
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
