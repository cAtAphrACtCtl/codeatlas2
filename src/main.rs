use std::{
    env::args,
    path::{self, Path, PathBuf},
    vec,
};
use std::fs::DirEntry;
use async_walkdir::{Filtering, WalkDir};
use clap::{Arg, ArgAction, Command, arg, builder::Str};
use futures_lite::StreamExt;
use futures_lite::future::block_on;
use serde_json::to_string;

fn main() {
    let cmd = Command::new("atlas")
        .version("0.0.1")
        .author("cAtAphrACtCtl")
        .about("This the intro of the cli application")
        .args(&[
            Arg::new("get_symbol")
                .short('g')
                .long("get")
                .help("get symbol definition location"),
            Arg::new("build")
                .short('b')
                .long("build")
                .help("build info for a give repo")
                .action(ArgAction::Append)
                .num_args(2),
        ]);

    let matches = cmd.get_matches();

    if let Some(symbol) = matches.get_one::<String>("get_symbol") {
        get_symbol(symbol);
    }

    if let Some(args) = matches
        .get_many::<String>("build")
        .map(|s| s.map(|s| s.as_str()).collect::<Vec<&str>>())
    {
        let path = args.first().expect("msg");
        build(path);
    }
}

fn get_symbol(symbol: &String) -> String {
    println!("get_symbol: {symbol}");
    symbol.clone()
}

fn build(path: &str) -> bool {
    let output_path = "output";
    println!("build on: {path}, exported to {output_path}");

    let repo = Repo {
        name: path.to_string(),
        path: path.to_string(),
        files: vec![],
    };

    println!("repo name: {}, files: {:?}", repo.name, repo.files);

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

struct Repo {
    name: String,
    path: String,
    files: Vec<String>,
}

struct Symbol {
    language: String,
    name: String,
    kind: String,
    location: SymbolLocation,
}

struct SymbolLocation {
    file: String,
    line: usize,
    col: usize,
}
