use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};

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
                .args(&["dir", "repo"])
                .multiple(false),
        )
}

pub fn find_handle_commands(matches: &ArgMatches) {
    let symbol = matches
        .get_one::<String>("symbol")
        .expect("symbol is required");

    let scope = match (
        matches.get_one::<String>("dir"),
        matches.get_one::<String>("repo"),
        ) {
        (Some(dir), None) => FindScope::Dir(dir.to_string()),
        (None, Some(repo)) => FindScope::Repo(repo.to_string()),
        (_,_) => FindScope::All,
    };

    let mode = matches.get_flag("list");

    let query = FindQuery{
        symbol:symbol.to_string(),
        scope:scope,
        list_mode:mode,
    };

    find(query);
}

#[derive(Debug)]
enum FindScope {
    Dir(String),
    Repo(String),
    All,
}

#[derive(Debug)]
pub struct FindQuery {
    symbol: String,
    scope: FindScope,
    list_mode: bool,
}

pub fn find(query: FindQuery) -> String {
    println!("get_symbol: {:?}", query);
    query.symbol.clone()
}
