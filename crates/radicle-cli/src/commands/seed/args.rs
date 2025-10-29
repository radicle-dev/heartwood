use std::time;

use clap::Parser;

use nonempty::NonEmpty;
use radicle::node::policy::Scope;
use radicle::prelude::*;

use crate::node::SyncSettings;
use crate::terminal;

const ABOUT: &str = "Manage repository seeding policies";

const LONG_ABOUT: &str = r#"
The `seed` command, when no Repository ID is provided, will list the
repositories being seeded.

When a Repository ID is provided it updates or creates the seeding policy for
that repository. To delete a seeding policy, use the `rad unseed` command.

When seeding a repository, a scope can be specified: this can be either `all` or
`followed`. When using `all`, all remote nodes will be followed for that repository.
On the other hand, with `followed`, only the repository delegates will be followed,
plus any remote that is explicitly followed via `rad follow <nid>`.
"#;

#[derive(Parser, Debug)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    #[arg(value_name = "RID", num_args = 1..)]
    pub(super) rids: Option<Vec<RepoId>>,

    /// Fetch repository after updating seeding policy
    #[arg(long, overrides_with("no_fetch"), hide(true))]
    fetch: bool,

    /// Do not fetch repository after updating seeding policy
    #[arg(long, overrides_with("fetch"))]
    no_fetch: bool,

    /// Fetch from the given node (may be specified multiple times)
    #[arg(long, value_name = "NID", action = clap::ArgAction::Append)]
    pub(super) from: Vec<NodeId>,

    /// Fetch timeout in seconds
    #[arg(long, short, value_name = "SECS", default_value_t = 9)]
    timeout: u64,

    /// Peer follow scope for this repository
    #[arg(
        long,
        default_value_t = Scope::All,
        value_parser = terminal::args::ScopeParser
    )]
    pub(super) scope: Scope,

    /// Verbose output
    #[arg(long, short)]
    pub(super) verbose: bool,
}

pub(super) enum Operation {
    List,
    Seed {
        rids: NonEmpty<RepoId>,
        should_fetch: bool,
        settings: SyncSettings,
        scope: Scope,
    },
}

impl From<Args> for Operation {
    fn from(args: Args) -> Self {
        let should_fetch = args.should_fetch();
        let timeout = args.timeout();
        let Args {
            rids, from, scope, ..
        } = args;
        match rids.and_then(NonEmpty::from_vec) {
            Some(rids) => Operation::Seed {
                rids,
                should_fetch,
                settings: SyncSettings::default().seeds(from).timeout(timeout),
                scope,
            },
            None => Self::List,
        }
    }
}

impl Args {
    fn timeout(&self) -> time::Duration {
        time::Duration::from_secs(self.timeout)
    }

    fn should_fetch(&self) -> bool {
        match (self.fetch, self.no_fetch) {
            (true, false) => true,
            (false, true) => false,
            // Default it to fetch
            (_, _) => true,
        }
    }
}
