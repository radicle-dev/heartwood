use std::path::PathBuf;
use std::time;

use clap::Parser;

use crate::node::SyncSettings;
use radicle::identity::doc::RepoId;
use radicle::identity::IdError;
use radicle::node::policy::Scope;
use radicle::prelude::*;

pub(crate) const ABOUT: &str = "Clone a Radicle repository";

const LONG_ABOUT: &str = r#"
The `clone` command will use your local node's routing table to find seeds from
which it can clone the repository.

For private repositories, use the `--seed` options, to clone directly
from known seeds in the privacy set."#;

/// Parse an RID, optionally stripping "rad://" prefix.
fn parse_rid(value: &str) -> Result<RepoId, IdError> {
    use std::str::FromStr as _;
    RepoId::from_str(value.strip_prefix("rad://").unwrap_or(value))
}

#[derive(Debug, Parser)]
pub(super) struct SyncArgs {
    /// Clone from this seed (may be specified multiple times).
    #[arg(short, long = "seed", value_name = "NID", action = clap::ArgAction::Append)]
    seeds: Vec<NodeId>,

    /// Timeout for fetching repository in seconds.
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

#[derive(Clone, Debug)]
struct ScopeParser;

impl clap::builder::TypedValueParser for ScopeParser {
    type Value = Scope;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        <Scope as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new(
            [PossibleValue::new("all"), PossibleValue::new("followed")].into_iter(),
        ))
    }
}

#[derive(Debug, Parser)]
#[clap(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// ID of the repository to clone
    #[arg(value_name = "RID", value_parser = parse_rid)]
    pub(super) repo: RepoId,

    /// The target directory for the repository to be cloned into.
    #[arg(value_name = "PATH")]
    pub(super) directory: Option<PathBuf>,

    /// Follow scope
    #[arg(long, default_value_t = Scope::All, value_name = "SCOPE", value_parser = ScopeParser)]
    pub(super) scope: Scope,

    #[clap(flatten)]
    pub(super) sync: SyncArgs,

    /// Make a bare repository.
    #[arg(long)]
    pub(super) bare: bool,

    // We keep this flag here for consistency though it doesn't have any effect,
    // since the command is fully non-interactive.
    #[arg(long, hide = true)]
    pub(super) no_confirm: bool,
}
