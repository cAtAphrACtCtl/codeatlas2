use std::fmt::Debug;
use clap::ArgMatches;

pub(crate) trait Query : Debug + Eq + PartialEq
{
    fn parse(matches: &ArgMatches) -> Self;
}
