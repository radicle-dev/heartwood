use std::{fmt::Display, str::FromStr};

use clap::{Parser, Subcommand, ValueEnum};
use radicle::{node::notifications::NotificationId, prelude::RepoId};

const ABOUT: &str = "Manage your Radicle notifications";

const LONG_ABOUT: &str = r#"
By default, this command lists all items in your inbox.
If your working directory is a Radicle repository, it only shows items
belonging to this repository, unless `--all` is used.

The `show` subcommand takes a notification ID (which can be found in
the output of the `list` subcommand) and displays the information related to that
notification. This will mark the notification as read.

The `clear` subcommand will clear all notifications with given IDs,
or all notifications if no IDs are given. Cleared notifications are
deleted and cannot be restored.
"#;

#[derive(Clone, Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(super) command: Option<Command>,

    #[clap(flatten)]
    pub(super) empty: EmptyArgs,
}

#[derive(Subcommand, Clone, Debug)]
pub(super) enum Command {
    /// List all items in your inbox
    List(ListArgs),
    /// Show a notification
    ///
    /// The NOTIFICATION_ID can be found by listing the items in your inbox
    ///
    /// Showing a notification will mark that notification as read
    Show {
        /// The notification to display
        #[arg(value_name = "NOTIFICATION_ID")]
        id: NotificationId,
    },
    /// Clear notifications
    ///
    /// This will clear all given notifications
    ///
    /// If no notifications are specified then all notifications are cleared
    Clear(ClearArgs),
}

#[derive(Parser, Clone, Copy, Debug)]
pub(super) struct EmptyArgs {
    /// Sort by column
    #[arg(long, value_enum, default_value_t, hide = true)]
    sort_by: SortBy,

    /// Reverse the list
    #[arg(short, long, hide = true)]
    reverse: bool,

    /// Show any updates that were not recognized
    #[arg(long, hide = true)]
    show_unknown: bool,

    /// Operate on a given repository [default: cwd]
    #[arg(value_name = "RID")]
    #[arg(long, hide = true)]
    repo: Option<RepoId>,

    /// Operate on all repositories
    #[arg(short, long, conflicts_with = "repo", hide = true)]
    all: bool,
}

#[derive(Parser, Clone, Copy, Debug)]
pub(super) struct ListArgs {
    /// Sort by column
    #[arg(long, value_enum, default_value_t)]
    pub(super) sort_by: SortBy,

    /// Reverse the list
    #[arg(short, long)]
    pub(super) reverse: bool,

    /// Show any updates that were not recognized
    #[arg(long)]
    pub(super) show_unknown: bool,

    /// Operate on a given repository [default: cwd]
    #[arg(long, value_name = "RID")]
    pub(super) repo: Option<RepoId>,

    /// Operate on all repositories
    #[arg(short, long, conflicts_with = "repo")]
    pub(super) all: bool,
}

impl From<ListArgs> for ListMode {
    fn from(args: ListArgs) -> Self {
        if args.all {
            assert!(args.repo.is_none());
            return Self::All;
        }

        if let Some(repo) = args.repo {
            return Self::ByRepo(repo);
        }

        Self::Contextual
    }
}

impl From<EmptyArgs> for ListArgs {
    fn from(
        EmptyArgs {
            sort_by,
            reverse,
            show_unknown,
            repo,
            all,
        }: EmptyArgs,
    ) -> Self {
        Self {
            sort_by,
            reverse,
            show_unknown,
            repo,
            all,
        }
    }
}

#[derive(Parser, Clone, Debug)]
pub(super) struct ClearArgs {
    /// Operate on a given repository [default: cwd]
    #[arg(long, value_name = "RID")]
    repo: Option<RepoId>,

    /// Operate on all repositories
    #[arg(short, long, conflicts_with = "repo")]
    all: bool,

    /// A list of notifications to clear
    ///
    /// The --repo or --all options are ignored when the notification ID's are
    /// specified
    #[arg(value_name = "NOTIFICATION_ID")]
    ids: Option<Vec<NotificationId>>,
}

impl From<ClearArgs> for ClearMode {
    fn from(ClearArgs { repo, all, ids }: ClearArgs) -> Self {
        if let Some(ids) = ids {
            return Self::ByNotifications(ids);
        }

        if all {
            assert!(repo.is_none());
            return Self::All;
        }

        if let Some(repo) = repo {
            return Self::ByRepo(repo);
        }

        Self::Contextual
    }
}

#[derive(ValueEnum, Clone, Copy, Default, Debug)]
pub enum SortBy {
    Id,
    #[default]
    Timestamp,
}

impl Display for SortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id => write!(f, "rowid"),
            Self::Timestamp => write!(f, "timestamp"),
        }
    }
}

impl FromStr for SortBy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "id" => Ok(Self::Id),
            "timestamp" => Ok(Self::Timestamp),
            _ => Err(format!("'{s}' is not a valid sort by column")),
        }
    }
}

pub(super) enum ListMode {
    /// List the notifications of the current repository, if in a working
    /// directory, otherwise all the repositories.
    Contextual,
    /// List the notifications for a all repositories.
    All,
    /// List the notifications for a specific repository.
    ByRepo(RepoId),
}

pub(super) enum ClearMode {
    /// Clear the specified notifications.
    ///
    /// Note that this does not require a `RepoId` since the IDs are globally
    /// unique due to the use of a single sqlite table.
    ByNotifications(Vec<NotificationId>),
    /// Clear the notifications of a specific repository.
    ByRepo(RepoId),
    /// Clear all notifications of all repositories.
    All,
    /// Clear the notifications of the current repository, only if in a working
    /// directory.
    Contextual,
}
