use async_walkdir::{Filtering, WalkDir};
use futures_lite::future::block_on;
use futures_lite::stream::StreamExt;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 1 {
        println!("Usage: {} <root>", args[0]);
        return;
    }
    let root = args
        .get(1).unwrap();

    let import_regex =
        Regex::new(r#"^\s*(use|import|require).*['"]([^'"]+)['"]"#)
            .expect("Failed to compile regex");

    let mut nodes = HashSet::new();
    let mut edges = HashSet::new();
    block_on(async {
        let mut entries = WalkDir::new(root).filter(|entry| async move {
            let p = entry.path();

            let path_str = p.to_string_lossy();
            if p.is_dir() {
                if path_str.contains("/target/") ||path_str.contains("/.git/"){
                    return Filtering::IgnoreDir;
                }

                return Filtering::Continue;
            }
            if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                return Filtering::Continue;
            }

            Filtering::Ignore
        });

        loop {
            match entries.next().await {
                Some(Ok(entry)) => {
                    let path = entry.path();
                    if !path.is_file() || path.extension() != Some("rs".as_ref()) {
                        continue;
                    }

                    nodes.insert(entry.path().clone());

                    if let Ok(content) = std::fs::read_to_string(&entry.path()) {
                        import_regex.captures_iter(&content).for_each(|cap| {
                            let from = &cap[1];
                            let to = &cap[2];
                            let edge = Edge {
                                from: from.to_string(),
                                to: to.to_string(),
                            };
                            edges.insert(edge);
                        });
                    }
                }
                Some(Err(e)) => {
                    eprintln!("error: {}", e);
                    break;
                }
                None => break,
            }
        }
    });

    let graph = Graph {
        edges: edges.into_iter().collect(),
        nodes: nodes
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    };

    let json = serde_json::to_string_pretty(&graph).expect("Failed to serialize graph to JSON");
    println!("{}", json);
}

#[derive(Serialize, Debug, PartialEq, Eq, Hash)]
struct Edge {
    from: String,
    to: String,
}

#[derive(Serialize, Debug, PartialEq, Eq, Hash)]
struct Graph {
    edges: Vec<Edge>,
    nodes: Vec<String>,
}
