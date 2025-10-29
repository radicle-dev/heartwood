use clap::{Parser, Subcommand};

use radicle::git;
use radicle::git::fmt::RefString;
use radicle::node::NodeId;

use crate::terminal as term;

const ABOUT: &str = "Manage a repository's remotes";

#[derive(Parser, Debug)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(super) command: Option<Command>,

    /// Arguments for the empty subcommand.
    /// Will fall back to [`Command::List`].
    #[clap(flatten)]
    pub(super) empty: EmptyArgs,
}

#[derive(Subcommand, Debug)]
pub(super) enum Command {
    /// Add a Git remote for the provided NID
    #[clap(alias = "a")]
    Add {
        /// The DID or NID of the remote to add
        #[arg(value_parser = term::args::parse_nid)]
        nid: NodeId,

        /// Override the name of the Git remote
        ///
        /// [default: <ALIAS>@<NID>]
        #[arg(long, short, value_name = "REMOTE", value_parser = parse_refstr)]
        name: Option<RefString>,

        #[clap(flatten)]
        fetch: FetchArgs,

        #[clap(flatten)]
        sync: SyncArgs,
    },
    /// Remove the Git remote identified by REMOTE
    #[clap(alias = "r")]
    Rm {
        /// The name of the remote to delete
        #[arg(value_name = "REMOTE", value_parser = parse_refstr)]
        name: RefString,
    },
    /// List the stored remotes
    ///
    /// Filter the listed remotes using the provided options
    #[clap(alias = "l")]
    List(ListArgs),
}

#[derive(Parser, Debug)]
pub(super) struct FetchArgs {
    /// Fetch the remote from local storage (default)
    #[arg(long, conflicts_with = "no_fetch")]
    fetch: bool,

    /// Do not fetch the remote from local storage
    #[arg(long)]
    no_fetch: bool,
}

impl FetchArgs {
    pub(super) fn should_fetch(&self) -> bool {
        let Self { fetch, no_fetch } = self;
        *fetch || !no_fetch
    }
}

#[derive(Parser, Debug)]
pub(super) struct SyncArgs {
    /// Sync the remote refs from the network (default)
    #[arg(long, conflicts_with = "no_sync")]
    sync: bool,

    /// Do not sync the remote refs from the network
    #[arg(long)]
    no_sync: bool,
}

impl SyncArgs {
    pub(super) fn should_sync(&self) -> bool {
        let Self { sync, no_sync } = self;
        *sync || !no_sync
    }
}

#[derive(Parser, Clone, Copy, Debug)]
#[group(multiple = false)]
pub struct ListArgs {
    /// Show all remotes in both the Radicle storage and the working copy
    #[arg(long)]
    all: bool,

    /// Show all remotes that are listed in the working copy
    #[arg(long)]
    tracked: bool,

    /// Show all remotes that are listed in the Radicle storage
    #[arg(long)]
    untracked: bool,
}

impl From<ListArgs> for ListOption {
    fn from(
        ListArgs {
            all,
            tracked,
            untracked,
        }: ListArgs,
    ) -> Self {
        match (all, tracked, untracked) {
            (true, false, false) => Self::All,
            (false, true, false) | (false, false, false) => Self::Tracked,
            (false, false, true) => Self::Untracked,
            _ => unreachable!(),
        }
    }
}

pub(super) enum ListOption {
    /// Show all remotes in both the Radicle storage and the working copy
    All,
    /// Show all remotes that are listed in the working copy
    Tracked,
    /// Show all remotes that are listed in the Radicle storage
    Untracked,
}

#[derive(Parser, Clone, Copy, Debug)]
#[group(multiple = false)]
pub(super) struct EmptyArgs {
    #[arg(long, hide = true)]
    all: bool,

    #[arg(long, hide = true)]
    tracked: bool,

    #[arg(long, hide = true)]
    untracked: bool,
}

impl From<EmptyArgs> for ListArgs {
    fn from(args: EmptyArgs) -> Self {
        Self {
            all: args.all,
            tracked: args.tracked,
            untracked: args.untracked,
        }
    }
}

fn parse_refstr(refstr: &str) -> Result<RefString, git::fmt::Error> {
    RefString::try_from(refstr)
}
