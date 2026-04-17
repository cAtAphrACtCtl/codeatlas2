mod repo;
mod search;

use async_walkdir::{Filtering, WalkDir};
use clap::{Arg, ArgGroup, Command};
use futures_lite::StreamExt;
use futures_lite::future::block_on;
use std::{
    env,
    env::args,
    path::{self, Path, PathBuf},
    vec,
};
use crate::repo::repo::{repo_commands, repo_handle_commands};
use crate::search::find::{find_commands, find_handle_commands};

fn main() {
    let path = env::current_dir().expect("failed to get current directory");

    let cmd = Command::new("atlas")
        .version("0.0.1")
        .author("cAtAphrACtCtl")
        .about("This the intro of the cli application")
        .subcommands([
            repo_commands(),
            find_commands(),
        ]);

    let matches = cmd.get_matches();
    match matches.subcommand() {
        Some(("repo", sub_matches)) => repo_handle_commands(sub_matches),
        Some(("find", sub_matches)) => find_handle_commands(sub_matches),
        _ => unreachable!("unknown subcommand"),
    }
}
