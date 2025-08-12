#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Argument parsing for the `radicle-issue` command

use std::str::FromStr;

use clap::{Parser, Subcommand};
use clap_complete::ArgValueCompleter;
use radicle::{
    cob::{thread, Label, Reaction},
    identity::{did::DidError, Did, RepoId},
    issue::{CloseReason, State},
};

use crate::{commands::hints, git::Rev, terminal::patch::Message};

/// Command line Peer argument.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Assigned {
    /// Filter issues assigned to the local `NID`
    #[default]
    Me,
    /// Filter issues assigned to the given `DID`
    Peer(Did),
}

/// Commands and arguments for the `radicle issue` command
#[derive(Parser, Debug)]
pub struct Args {
    /// Subcommand for `radicle issue`
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

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
    #[clap(global = true)]
    pub(crate) repo: Option<RepoId>,

    /// Verbose output
    #[arg(long, short)]
    #[clap(global = true)]
    pub(crate) verbose: bool,
}

/// Commands to create, view, and edit Radicle issues
#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Delete an issue
    Delete {
        /// The issue to delete
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
        id: Rev,
    },

    /// Edit an issue
    Edit {
        /// The issue to edit
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
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
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
        id: Rev,

        /// The emoji reaction to react with
        #[arg(long = "emoji")]
        #[arg(value_name = "CHAR")]
        reaction: Option<Reaction>,

        /// Optionally react to a given comment in the issue
        #[arg(long = "to")]
        #[arg(value_name = "COMMENT_ID")]
        comment_id: Option<thread::CommentId>,
    },

    /// Manage assignees of an issue
    Assign {
        /// The issue to assign a DID to
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times)
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        #[arg(add = ArgValueCompleter::new(hints::dids_completer))]
        add: Vec<Did>,

        /// Delete an assignee from the issue (may be specified multiple times)
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        #[arg(add = ArgValueCompleter::new(hints::dids_completer))]
        delete: Vec<Did>,
    },

    /// Update labels on an issue
    Label {
        /// The issue to label
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
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
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
        id: Rev,

        /// The body of the comment
        #[arg(long, short)]
        #[arg(value_name = "MESSAGE")]
        message: Message,

        /// Optionally, the comment to reply to. If not specified, the comment
        /// will be in reply to the issue itself.
        #[arg(long, name = "COMMENT_ID_TO_REPLY")]
        reply_to: Option<Rev>,

        /// The comment to edit (if any)
        #[arg(long, name = "COMMENT_ID_TO_EDIT")]
        edit: Option<Rev>,
    },

    /// Show a specific issue
    Show {
        /// The issue to display
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
        id: Rev,
    },

    /// Re-cache all issues that can be found in Radicle storage
    Cache {
        /// Optionally choose an issue to re-cache
        #[arg(value_name = "ISSUE_ID")]
        id: Option<Rev>,

        /// Operate on storage
        #[arg(long)]
        storage: bool,
    },

    /// Set the state of an issue
    State {
        /// The issue to be transitioned
        #[arg(value_name = "ISSUE_ID", add = ArgValueCompleter::new(hints::issue_ids_completer))]
        id: Rev,

        /// The desired target state
        #[clap(flatten)]
        target_state: StateArgs,
    },
}

impl Default for Command {
    fn default() -> Self {
        Self::List(ListArgs::default())
    }
}

/// Arguments for the [`Command::List`] subcommand.
#[derive(Parser, Debug)]
pub(crate) struct ListArgs {
    /// List issues assigned to <DID> (default: me)
    #[arg(long, name = "DID")]
    #[arg(default_missing_value = "me")]
    #[arg(num_args = 0..=1)]
    #[arg(require_equals = true)]
    #[arg(add = ArgValueCompleter::new(hints::assignee_dids_completer))]
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
#[group(id = "state", required = true, multiple = false)]
pub(crate) struct StateArgs {
    /// Change state to open
    #[arg(long)]
    #[arg(group = "state")]
    pub(crate) open: bool,

    /// Change state to closed
    #[arg(long)]
    #[arg(group = "state")]
    pub(crate) closed: bool,

    /// Change state to solved
    #[arg(long)]
    #[arg(group = "state")]
    pub(crate) solved: bool,
}

impl From<StateArgs> for StateArg {
    fn from(state: StateArgs) -> Self {
        // These are mutually exclusive, guaranteed by clap grouping
        match (state.open, state.closed, state.solved) {
            (true, _, _) => StateArg::Open,
            (_, true, _) => StateArg::Closed,
            (_, _, true) => StateArg::Solved,
            _ => unreachable!(),
        }
    }
}

/// Argument value for transition an issue to the given [`State`].
#[derive(Clone, Copy, Debug)]
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
