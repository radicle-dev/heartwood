use std::time;

#[allow(rustdoc::broken_intra_doc_links)]
use clap::Parser;

use radicle::git;
use radicle::git::fmt::RefString;
use radicle::prelude::{NodeId, RepoId};

const ABOUT: &str = "Wait for some state to be updated";

const LONG_ABOUT: &str = r#"
Watches a Git reference, and optionally exits when it reaches a target value.
If no target value is passed, exits when the target changes."#;

fn parse_refstr(refstr: &str) -> Result<RefString, git::fmt::Error> {
    RefString::try_from(refstr)
}

#[derive(Parser, Debug)]
#[command(about = ABOUT, long_about = LONG_ABOUT,disable_version_flag = true)]
pub struct Args {
    /// The repository to watch, defaults to `rad .`
    #[arg(long)]
    pub(super) repo: Option<RepoId>,

    /// The fully-qualified Git reference (branch, tag, etc.) to watch
    ///
    /// [example value: 'refs/heads/master']
    #[arg(long, short, alias = "ref", value_name = "REF", value_parser = parse_refstr)]
    pub(super) refstr: git::fmt::RefString,

    /// The target OID (commit hash) that when reached, will cause the command to exit
    #[arg(long, short, value_name = "OID")]
    pub(super) target: Option<git::Oid>,

    /// The namespace under which this reference exists, defaults to the profiles' NID
    #[arg(long, short, value_name = "NID")]
    pub(super) node: Option<NodeId>,

    /// How often, in milliseconds, to check the reference target
    #[arg(long, short, value_name = "MILLIS", default_value_t = 1000)]
    interval: u64,

    /// Timeout, in milliseconds
    #[arg(long, value_name = "MILLIS")]
    timeout: Option<u64>,
}

impl Args {
    /// Provide the interval duration in milliseconds.
    pub(super) fn interval(&self) -> time::Duration {
        time::Duration::from_millis(self.interval)
    }

    /// Provide the timeout duration in milliseconds.
    pub(super) fn timeout(&self) -> time::Duration {
        time::Duration::from_millis(self.timeout.unwrap_or(u64::MAX))
    }
}

#[cfg(test)]
mod test {
    use super::Args;
    use clap::Parser;

    #[test]
    fn should_parse_ref_str() {
        let args = Args::try_parse_from(["watch", "--ref", "refs/heads/master"]);
        assert!(args.is_ok())
    }
}
