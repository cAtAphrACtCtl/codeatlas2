use clap::ArgMatches;
use std::fmt::Debug;

pub(crate) trait CommandQuery: Debug + Eq + PartialEq {
    fn parse(matches: &ArgMatches) -> Self;
}
