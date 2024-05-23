#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Argument parsing for the `radicle-issue` command

use std::str::FromStr;

use clap::{Parser, Subcommand, ValueEnum, ValueHint};
use radicle::{
    cob::{thread, Label, Reaction},
    identity::{did::DidError, Did, RepoId},
    issue::{cache::IssuesExt as _, CloseReason, Issues, State},
    storage::ReadStorage as _,
};

use crate::{git::Rev, terminal::patch::Message};

/// Command line Peer argument.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) enum Assigned {
    /// Filter issues assigned to the local `NID`
    #[default]
    Me,
    /// Filter issues assigned to the given `DID`
    Peer(Did),
}

/// Commands and arguments for the `radicle issue` command
#[derive(Parser, Debug)]
pub struct Args {
    /// Set of subcommands for `radicle issue`.
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,

    /// Don't print anything
    #[arg(short, long)]
    #[clap(global = true)]
    pub(crate) quiet: bool,

    /// Don't announce issue to peers
    #[arg(long)]
    #[arg(value_name = "no-announce")]
    #[clap(global = true)]
    pub(crate) no_announce: bool,

    /// Show only the issue header, hiding the comments
    #[arg(long)]
    #[clap(global = true)]
    pub(crate) header: bool,

    /// Optionally specify the repository to manage issues for
    #[arg(value_name = "RID")]
    #[arg(long, short)]
    pub(crate) repo: Option<RepoId>,
}

/// Commands to create, view, and edit Radicle issues
#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Delete an issue
    Delete {
        /// The issue to delete
        #[arg(value_name = "ISSUE_ID")]
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        id: Rev,
    },

    /// Edit an issue
    Edit {
        /// The issue to edit
        #[arg(value_name = "ISSUE_ID")]
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        id: Rev,

        /// The new title to set for the issue
        #[arg(long, short)]
        title: Option<String>,

        /// The new description to set for the issue
        #[arg(long, short)]
        description: Option<String>,
    },

    /// List issues, optionally filtering them
    List(ListArgs),

    /// Create a new issue
    Open {
        /// The new title of the issue
        #[arg(long, short)]
        title: Option<String>,

        /// The new description of the issue
        #[arg(long, short)]
        description: Option<String>,

        /// A set of labels to associate with the issue
        #[arg(long)]
        labels: Vec<Label>,

        /// A set of DIDs to assign to the issue
        #[arg(value_name = "DID")]
        #[arg(long)]
        assignees: Vec<Did>,
    },

    /// Add a reaction emoji to an issue or comment
    React {
        /// The issue to react to
        #[arg(value_name = "ISSUE_ID")]
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        id: Rev,

        /// The emoji reaction to react with
        #[arg(long = "emoji")]
        #[arg(value_name = "CHAR")]
        reaction: Reaction,

        /// Optionally react to a given comment in the issue
        #[arg(long = "to")]
        #[arg(value_name = "COMMENT_ID")]
        // TODO: Add dynamic hint for comment ids
        comment_id: Option<thread::CommentId>,
    },

    /// Manage assignees of an issue
    Assign {
        /// The issue to assign a DID to
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times)
        #[clap(value_hint = ValueHint::Dynamic(hints::dids))]
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Did>,

        /// Delete an assignee from the issue (may be specified multiple times)
        #[clap(value_hint = ValueHint::Dynamic(hints::dids))]
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Did>,
    },

    /// Update labels on an issue
    Label {
        /// The issue to label
        #[arg(value_name = "ISSUE_ID")]
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times)
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Label>,

        /// Delete an assignee from the issue (may be specified multiple times)
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Label>,
    },

    /// Add a comment to an issue.
    Comment {
        /// The issue to comment on
        #[arg(value_name = "ISSUE_ID")]
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        id: Rev,

        /// The body of the comment
        #[arg(long, short)]
        #[arg(value_name = "MESSAGE")]
        message: Message,

        /// Optionally, the comment to reply to. If not specified, the comment
        /// will be in reply to the issue itself.
        #[arg(long, name = "COMMENT_ID")]
        reply_to: Option<Rev>,
    },

    /// Show a specific issue
    Show {
        /// The issue to display
        #[arg(value_name = "ISSUE_ID")]
        #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
        id: Rev,

        /// Show the issue as Rust debug output
        #[arg(long)]
        debug: bool,
    },

    /// Re-cache all issues that can be found in Radicle storage
    Cache {
        /// Optionally choose an issue to re-cache
        #[arg(value_name = "ISSUE_ID")]
        id: Option<Rev>,
    },

    /// Set the state of an issue
    State(StateArgs),
}

impl Default for Commands {
    fn default() -> Self {
        Self::List(ListArgs::default())
    }
}

/// Arguments for the [`Command::List`] subcommand.
#[derive(Parser, Debug)]
pub(crate) struct ListArgs {
    /// List issues assigned to <DID> (default: me)
    #[clap(value_hint = ValueHint::Dynamic(hints::assignee_dids))]
    #[arg(long, name = "DID")]
    #[arg(default_missing_value = "me")]
    #[arg(num_args = 0..=1)]
    #[arg(require_equals = true)]
    pub(crate) assigned: Option<Assigned>,

    /// List all issues
    #[arg(long, group = "state")]
    all: bool,

    /// List only open issues (default)
    #[arg(long, group = "state")]
    open: bool,

    /// List only closed issues
    #[arg(long, group = "state")]
    closed: bool,

    /// List only solved issues
    #[arg(long, group = "state")]
    solved: bool,
}

impl Default for ListArgs {
    fn default() -> Self {
        Self {
            assigned: None,
            all: false,
            open: true,
            closed: false,
            solved: false,
        }
    }
}

impl From<ListArgs> for Option<State> {
    fn from(value: ListArgs) -> Self {
        if value.open {
            Some(State::Open)
        } else if value.closed {
            Some(State::Closed {
                reason: CloseReason::Other,
            })
        } else if value.solved {
            Some(State::Closed {
                reason: CloseReason::Solved,
            })
        } else {
            None
        }
    }
}

/// Arguments for the [`Command::State`] subcommand.
#[derive(Parser, Debug)]
pub(crate) struct StateArgs {
    /// The issue to be transitioned
    #[arg(value_name = "ISSUE_ID")]
    #[clap(value_hint = ValueHint::Dynamic(hints::issue_ids))]
    pub(crate) id: Rev,

    /// Transition the issue state
    #[arg(long, value_name = "STATE")]
    pub(crate) to: StateArg,
}

/// Argument value for transition an issue to the given [`State`].
#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum StateArg {
    /// Open issues.
    /// Maps to [`State::Open`].
    Open,
    /// Closed issues.
    /// Maps to [`State::Closed`] and [`CloseReason::Other`].
    Closed,
    /// Solved issues.
    /// Maps to [`State::Closed`] and [`CloseReason::Solved`].
    Solved,
}

impl From<StateArg> for State {
    fn from(value: StateArg) -> Self {
        match value {
            StateArg::Open => Self::Open,
            StateArg::Closed => Self::Closed {
                reason: CloseReason::Other,
            },
            StateArg::Solved => Self::Closed {
                reason: CloseReason::Solved,
            },
        }
    }
}

impl FromStr for Assigned {
    type Err = DidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "me" {
            Ok(Assigned::Me)
        } else {
            let value = s.parse::<Did>()?;
            Ok(Assigned::Peer(value))
        }
    }
}

/// Provide auto-completion hints for CLI usage.
mod hints {
    use super::*;

    /// List the `DID`s associated with the current repository, and are assigned
    /// to any issue, filtering by the `prefix`.
    pub fn assignee_dids(prefix: &str) -> Option<Vec<String>> {
        let (_, rid) = radicle::rad::cwd().ok()?;
        radicle::Profile::load()
            .ok()
            .and_then(|profile| profile.storage.repository(rid).ok())
            .and_then(|repo| {
                Issues::open(&repo).ok().and_then(|issues| {
                    issues
                        .all()
                        .map(|issues| {
                            issues
                                .flat_map(|issue| {
                                    issue.map_or(vec![], |(_, issue)| {
                                        issue.assignees().cloned().collect::<Vec<_>>()
                                    })
                                })
                                .filter_map(|did| {
                                    let did = did.to_human();
                                    did.starts_with(prefix).then_some(did)
                                })
                                .collect::<Vec<_>>()
                        })
                        .ok()
                })
            })
    }

    /// List the `IssueId`s associated with the current repository, filtered by the `prefix`.
    pub fn issue_ids(prefix: &str) -> Option<Vec<String>> {
        let (_, rid) = radicle::rad::cwd().ok()?;
        let profile = radicle::Profile::load().ok()?;
        let repo = profile.storage.repository(rid).ok()?;
        let issues = profile.issues(&repo).ok()?;
        let ids = issues.ids(prefix).ok()?;
        Some(
            ids.filter_map(|result| result.ok().map(|id| id.to_string()))
                .collect(),
        )
    }

    /// List the `DID`s associated with the current repository, filtered by the `prefix`.
    // TODO: we could make this more like a fuzzy search
    pub fn dids(prefix: &str) -> Option<Vec<String>> {
        let (_, rid) = radicle::rad::cwd().ok()?;
        let profile = radicle::Profile::load().ok()?;
        let repo = profile.storage.repository(rid).ok()?;
        let ids = repo.remote_ids().ok()?;
        Some(
            ids.filter_map(|nid| {
                let nid = nid.ok()?;
                let did = Did::from(nid).to_human();
                did.starts_with(prefix).then_some(did)
            })
            .collect(),
        )
    }
}
