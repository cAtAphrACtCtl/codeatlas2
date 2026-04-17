mod query;
mod repo;
mod search;

use crate::repo::repo::{repo_commands, repo_handle_commands};
use crate::search::find::{find_commands, find_handle_command};
use clap::Command;
use futures_lite::StreamExt;
use std::env;

fn main() {
    let path = env::current_dir().expect("failed to get current directory");

    let cmd = Command::new("atlas")
        .version("0.0.1")
        .author("cAtAphrACtCtl")
        .about("This the intro of the cli application")
        .subcommands([repo_commands(), find_commands()]);

    let matches = cmd.get_matches();
    match matches.subcommand() {
        Some(("repo", sub_matches)) => repo_handle_commands(sub_matches),
        Some(("find", sub_matches)) => find_handle_command(sub_matches),
        _ => unreachable!("unknown subcommand"),
    }
}
