use async_walkdir::{Filtering, WalkDir};
use clap::{Arg, ArgMatches, Command};
use futures_lite::StreamExt;
use futures_lite::future::block_on;
use std::path::Path;

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
                    Arg::new("path")
                        .short('p')
                        .help("path to the repo")
                        .required(true),
                )
                .arg(
                    Arg::new("name")
                        .short('n')
                        .help("name to the repo")
                        .required(true),
                ),
            Command::new("delete").arg(Arg::new("name").help("name to the repo").required(true)),
        ])
}

pub fn repo_handle_commands(matches: &ArgMatches) {
    match matches.subcommand() {
        Some(("add", sub_matches)) => {
            println!("add");
        }
        Some(("delete", sub_matches)) => {
            println!("delete");
        }
        _ => unreachable!("unknown subcommand"),
    }
}

pub fn add(path: &str, name: &str) -> bool {
    let output_path = "output";
    println!("build on: {path}, exported to {output_path}");
    let files = walk_dir(path);

    let repo = Repo::new(path.to_string(), path.to_string(), files);
    println!("repo :{:?}", repo);

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
