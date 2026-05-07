mod config;
mod extraction;
mod query;
mod repo;
mod search;
#[cfg(test)]
mod test_support;

use crate::repo::repo::{repo_commands, repo_handle_commands};
use crate::search::find::{find_commands, find_handle_command};
use clap::Command;
use std::time::Instant;

fn main() {
    let cmd = Command::new("atlas")
        .version("0.0.1")
        .author("cAtAphrACtCtl")
        .about("This the intro of the cli application")
        .subcommands([repo_commands(), find_commands()]);

    let matches = cmd.get_matches();

    let now = Instant::now();
    match matches.subcommand() {
        Some(("repo", sub_matches)) => repo_handle_commands(sub_matches),
        Some(("find", sub_matches)) => find_handle_command(sub_matches),
        _ => unreachable!("unknown subcommand"),
    }

    let elapsed = Instant::now() - now;
    println!("finished with duration = {:?}", elapsed);
}
