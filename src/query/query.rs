use std::fmt::Debug;
use clap::ArgMatches;

pub(crate) trait CommandQuery: Debug + Eq + PartialEq
{
    fn parse(matches: &ArgMatches) -> Self;
}
