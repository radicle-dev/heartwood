use std::path::PathBuf;
use std::time;

use clap::Parser;

use crate::node::SyncSettings;
use radicle::identity::doc::RepoId;
use radicle::identity::IdError;
use radicle::node::policy::Scope;
use radicle::prelude::*;

use crate::terminal;

const ABOUT: &str = "Clone a Radicle repository";

const LONG_ABOUT: &str = r#"
The `clone` command will use your local node's routing table to find seeds from
which it can clone the repository.

For private repositories, use the `--seed` options, to clone directly
from known seeds in the privacy set."#;

/// Parse an RID, optionally stripping "rad://" prefix.
fn parse_rid(value: &str) -> Result<RepoId, IdError> {
    value.strip_prefix("rad://").unwrap_or(value).parse()
}

#[derive(Debug, Parser)]
pub(super) struct SyncArgs {
    /// Clone from this seed (may be specified multiple times)
    #[arg(short, long = "seed", value_name = "NID", action = clap::ArgAction::Append)]
    seeds: Vec<NodeId>,

    /// Timeout for fetching repository in seconds
    #[arg(long, default_value_t = 9, value_name = "SECS")]
    timeout: usize,
}

impl From<SyncArgs> for SyncSettings {
    fn from(args: SyncArgs) -> Self {
        SyncSettings {
            timeout: time::Duration::from_secs(args.timeout as u64),
            seeds: args.seeds.into_iter().collect(),
            ..SyncSettings::default()
        }
    }
}

#[derive(Debug, Parser)]
#[clap(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// ID of the repository to clone
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, rad://z3Tr6bC7ctEg2EHmLvknUr29mEDLH]
    #[arg(value_name = "RID", value_parser = parse_rid)]
    pub(super) repo: RepoId,

    /// The target directory for the repository to be cloned into
    #[arg(value_name = "PATH")]
    pub(super) directory: Option<PathBuf>,

    /// Follow scope
    #[arg(
        long,
        default_value_t = Scope::All,
        value_parser = terminal::args::ScopeParser
    )]
    pub(super) scope: Scope,

    #[clap(flatten)]
    pub(super) sync: SyncArgs,

    /// Make a bare repository
    #[arg(long)]
    pub(super) bare: bool,

    // We keep this flag here for consistency though it doesn't have any effect,
    // since the command is fully non-interactive.
    #[arg(long, hide = true)]
    pub(super) no_confirm: bool,
}

#[cfg(test)]
mod test {
    use super::Args;
    use clap::Parser;

    #[test]
    fn should_parse_rid_non_urn() {
        let args = Args::try_parse_from(["clone", "z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_parse_rid_urn() {
        let args = Args::try_parse_from(["clone", "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_parse_rid_url() {
        let args = Args::try_parse_from(["clone", "rad://z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }
}
