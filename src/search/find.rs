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

pub fn find_handle_command(matches: &ArgMatches) {
    let query = find_parse_command(matches);
    find(query);
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

#[derive(Debug)]
#[derive(Eq, PartialEq)]
enum FindScope {
    Dir(String),
    Repo(String),
    All,
}

#[derive(Debug)]
#[derive(Eq, PartialEq)]
pub struct FindQuery {
    symbol: String,
    scope: FindScope,
    list_mode: bool,
}

pub fn find(query: FindQuery) -> String {
    println!("get_symbol: {:?}", query);
    query.symbol.clone()
}

#[cfg(test)]
mod tests{
    use crate::search::find::{find_commands, find_parse_command, FindQuery, FindScope};

    fn parse(args: Vec<&str>) -> FindQuery {
        let matches = find_commands()
            .try_get_matches_from(args.as_slice())
            .expect("failed to parse args");

        find_parse_command(&matches)
    }

    #[test]
    fn test_find_parse_symbol_only(){
        let expected_query = FindQuery {
            symbol:String::from("a"),
            scope:FindScope::All,
            list_mode: false,
        };

        let query = parse(vec!["find", "a"]);
        assert_eq!(expected_query, query);
    }

    #[test]
    fn test_find_parse_symbol_and_dir(){
        let expected_query = FindQuery {
            symbol:String::from("a"),
            scope:FindScope::Dir(String::from("path")),
            list_mode: false,
        };

        let query = parse(vec!["find", "a", "-d", "path"]);
        assert_eq!(expected_query, query);
    }

    #[test]
    fn test_find_parse_symbol_with_dir_and_list(){
        let expected_query = FindQuery {
            symbol:String::from("a"),
            scope:FindScope::Repo(String::from("repo")),
            list_mode: true,
        };

        let query = parse(vec!["find", "a", "-r", "repo", "-l"]);
        assert_eq!(expected_query, query);
    }
}
