use std::str::FromStr;
use std::time;

use clap::{Parser, Subcommand, ValueEnum};

use radicle::{
    node::{NodeId, sync},
    prelude::RepoId,
    storage::refs,
};

use crate::common_args::{
    ABOUT_FETCH_SIGNED_REFERENCES_FEATURE_LEVEL_MINIMUM, SignedReferencesFeatureLevel,
    SignedReferencesFeatureLevelParser,
};
use crate::node::SyncSettings;

const ABOUT: &str = "Sync repositories to the network";

const LONG_ABOUT: &str = r#"
By default, the current repository is synchronized both ways.
If an <RID> is specified, that repository is synced instead.

The process begins by fetching changes from connected seeds,
followed by announcing local refs to peers, thereby prompting
them to fetch from us.

When `--fetch` is specified, any number of seeds may be given
using the `--seed` option, eg. `--seed <NID>@<ADDR>:<PORT>`.

When `--replicas` is specified, the given replication factor will try
to be matched. For example, `--replicas 5` will sync with 5 seeds.

The synchronization process can be configured using `--replicas <MIN>` and
`--replicas-max <MAX>`. If these options are used independently, then the
replication factor is taken as the given `<MIN>`/`<MAX>` value. If the
options are used together, then the replication factor has a minimum and
maximum bound.

For fetching, the synchronization process will be considered successful if
at least `<MIN>` seeds were fetched from *or* all preferred seeds were
fetched from. If `<MAX>` is specified then the process will continue and
attempt to sync with `<MAX>` seeds.

For reference announcing, the synchronization process will be considered
successful if at least `<MIN>` seeds were pushed to *and* all preferred
seeds were pushed to.

When `--fetch` or `--announce` are specified on their own, this command
will only fetch or announce.

If `--inventory` is specified, the node's inventory is announced to
the network. This mode does not take an `<RID>`.
"#;

#[derive(Parser, Debug)]
#[clap(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    #[clap(subcommand)]
    pub(super) command: Option<Command>,

    #[clap(flatten)]
    pub(super) sync: SyncArgs,

    /// Enable debug information when synchronizing
    #[arg(long)]
    pub(super) debug: bool,

    /// Enable verbose information when synchronizing
    #[arg(long, short)]
    pub(super) verbose: bool,
}

#[derive(Parser, Debug)]
pub(super) struct SyncArgs {
    /// Enable fetching [default: true]
    ///
    /// Providing `--announce` without `--fetch` will disable fetching
    #[arg(long, short, conflicts_with = "inventory")]
    fetch: bool,

    /// Enable announcing [default: true]
    ///
    /// Providing `--fetch` without `--announce` will disable announcing
    #[arg(long, short, conflicts_with = "inventory")]
    announce: bool,

    /// Synchronize with the given node (may be specified multiple times)
    #[arg(
        long = "seed",
        value_name = "NID",
        action = clap::ArgAction::Append,
        conflicts_with = "inventory",
    )]
    seeds: Vec<NodeId>,

    /// How long to wait while synchronizing
    ///
    /// Valid arguments are for example "10s", "5min" or "2h 37min"
    #[arg(
        long,
        short,
        default_value = "9s",
        value_parser = humantime::parse_duration,
        conflicts_with = "inventory"
    )]
    timeout: std::time::Duration,

    /// The repository to perform the synchronizing for [default: cwd]
    rid: Option<RepoId>,

    /// Synchronize with a specific number of seeds
    ///
    /// The value must be greater than zero
    #[arg(
        long,
        short,
        value_name = "COUNT",
        value_parser = replicas_non_zero,
        conflicts_with = "inventory",
        default_value_t = radicle::node::sync::DEFAULT_REPLICATION_FACTOR,
    )]
    replicas: usize,

    /// Synchronize with an upper bound number of seeds
    ///
    /// The value must be greater than zero
    #[arg(
        long,
        value_name = "COUNT",
        value_parser = replicas_non_zero,
        conflicts_with = "inventory",
    )]
    max_replicas: Option<usize>,

    /// Enable announcing inventory [default: false]
    ///
    /// `--inventory` is a standalone mode and is not compatible with the other
    /// options
    ///
    /// <RID> is ignored with `--inventory`
    #[arg(long, short)]
    inventory: bool,

    #[arg(
        long,
        requires = "fetch",
        value_parser = SignedReferencesFeatureLevelParser,
        help = ABOUT_FETCH_SIGNED_REFERENCES_FEATURE_LEVEL_MINIMUM
    )]
    signed_refs_feature_level: Option<SignedReferencesFeatureLevel>,
}

impl SyncArgs {
    fn direction(&self) -> SyncDirection {
        match (self.fetch, self.announce) {
            (true, true) | (false, false) => SyncDirection::Both,
            (true, false) => SyncDirection::Fetch,
            (false, true) => SyncDirection::Announce,
        }
    }

    fn timeout(&self) -> time::Duration {
        self.timeout
    }

    fn replication(&self) -> sync::ReplicationFactor {
        match (self.replicas, self.max_replicas) {
            (min, None) => sync::ReplicationFactor::must_reach(min),
            (min, Some(max)) => sync::ReplicationFactor::range(min, max),
        }
    }
}

#[derive(Subcommand, Debug)]
pub(super) enum Command {
    /// Display the sync status of a repository
    #[clap(alias = "s")]
    Status {
        /// The repository to display the status for [default: cwd]
        rid: Option<RepoId>,
        /// Sort the table by column
        #[arg(long, value_name = "FIELD", value_enum, default_value_t)]
        sort_by: SortBy,
    },
}

/// Sort the status table by the provided field
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SortBy {
    /// The NID of the entry
    Nid,
    /// The alias of the entry
    Alias,
    /// The status of the entry
    #[default]
    Status,
}

impl FromStr for SortBy {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "nid" => Ok(Self::Nid),
            "alias" => Ok(Self::Alias),
            "status" => Ok(Self::Status),
            _ => Err("invalid `--sort-by` field"),
        }
    }
}

/// Whether we are performing a fetch/announce of a repository or only
/// announcing the node's inventory
pub(super) enum SyncMode {
    /// Fetch and/or announce a repositories references
    Repo {
        /// The repository being synchronized
        rid: Option<RepoId>,
        /// The settings for fetch/announce
        settings: SyncSettings,
        /// The direction of the synchronization
        direction: SyncDirection,
    },
    /// Announce the node's inventory
    Inventory,
}

impl From<SyncArgs> for SyncMode {
    fn from(args: SyncArgs) -> Self {
        if args.inventory {
            Self::Inventory
        } else {
            assert!(!args.inventory);
            let direction = args.direction();
            let timeout = args.timeout();
            let replicas = args.replication();
            let feature_level = args.signed_refs_feature_level.map(refs::FeatureLevel::from);
            let mut settings = SyncSettings::default()
                .timeout(timeout)
                .replicas(replicas)
                .minimum_feature_level(feature_level);
            if !args.seeds.is_empty() {
                settings.seeds = args.seeds.into_iter().collect();
            }
            Self::Repo {
                rid: args.rid,
                settings,
                direction,
            }
        }
    }
}

/// The direction of the [`SyncMode`]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SyncDirection {
    /// Only fetching
    Fetch,
    /// Only announcing
    Announce,
    /// Both fetching and announcing
    Both,
}

fn replicas_non_zero(s: &str) -> Result<usize, String> {
    let r = usize::from_str(s).map_err(|_| format!("{s} is not a number"))?;
    if r == 0 {
        return Err(format!("{s} must be a value greater than zero"));
    }
    Ok(r)
}
