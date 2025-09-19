#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Argument parsing for the `radicle-issue` command

use std::str::FromStr;

use clap::{Parser, Subcommand};
use radicle::{
    cob::{Label, Reaction, Title},
    identity::{did::DidError, Did, RepoId},
    issue::{CloseReason, State},
};

use crate::{git::Rev, terminal::patch::Message};

/// Command line Peer argument.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Assigned {
    /// Filter issues assigned to the local DID
    #[default]
    Me,
    /// Filter issues assigned to the given DID
    Peer(Did),
}

/// Commands and arguments for the `radicle issue` command
#[derive(Parser, Debug)]
#[command(disable_version_flag = true)]
pub struct Args {
    /// Subcommand for `radicle issue`
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Do not print anything
    #[arg(short, long)]
    #[clap(global = true)]
    pub(crate) quiet: bool,

    /// Do not announce issue changes to the network
    #[arg(long)]
    #[arg(value_name = "no-announce")]
    #[clap(global = true)]
    pub(crate) no_announce: bool,

    /// Show only the issue header, hiding the comments
    #[arg(long)]
    #[clap(global = true)]
    pub(crate) header: bool,

    /// Operate on the given repository (default: cwd)
    #[arg(value_name = "RID")]
    #[arg(long, short)]
    #[clap(global = true)]
    pub(crate) repo: Option<RepoId>,

    /// Enable verbose output
    #[arg(long, short)]
    #[clap(global = true)]
    pub(crate) verbose: bool,
}

/// Commands to create, view, and edit Radicle issues
#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Add or delete assignees from an issue
    Assign {
        /// ID of the issue
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// Add an assignee (may be specified multiple times)
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Did>,

        /// Delete an assignee (may be specified multiple times)
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Did>,
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
    /// Add a comment to an issue
    Comment(CommentArgs),
    /// Edit the title and description of an issue
    Edit {
        /// ID of the issue
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// The new title to set
        #[arg(long, short)]
        title: Option<Title>,

        /// The new description to set
        #[arg(long, short)]
        description: Option<String>,
    },
    /// Delete an issue
    Delete {
        /// ID of the issue
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,
    },
    /// Add or delete labels from an issue
    Label {
        /// ID of the issue
        id: Rev,

        /// Add an assignee (may be specified multiple times)
        ///
        /// Note: --add takes precedence over --delete
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Label>,

        /// Delete an assignee (may be specified multiple times)
        ///
        /// Note: --add takes precedence over --delete
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Label>,
    },
    /// List issues, optionally filtering them
    List(ListArgs),
    /// Open a new issue
    Open {
        /// The title of the issue
        #[arg(long, short)]
        title: Option<Title>,

        /// The description of the issue
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
        /// ID of the issue
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// The emoji reaction
        #[arg(long = "emoji")]
        #[arg(value_name = "CHAR")]
        reaction: Option<Reaction>,

        /// Optionally react to a comment
        #[arg(long = "to")]
        #[arg(value_name = "COMMENT_ID")]
        comment_id: Option<Rev>,
    },
    /// Show a specific issue
    Show {
        /// ID of the issue
        id: Rev,
    },
    /// Transition the state of an issue
    State {
        /// ID of the issue
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// The desired target state
        #[clap(flatten)]
        target_state: StateArgs,
    },
}

impl Command {
    /// Returns `true` if the changes made by the command should announce to the
    /// network.
    pub(crate) fn should_announce_for(&self) -> bool {
        match self {
            Command::Open { .. }
            | Command::React { .. }
            | Command::State { .. }
            | Command::Delete { .. }
            | Command::Assign { .. }
            | Command::Label { .. }
            // Special handling for `--edit` will be removed in the future.
            | Command::Edit { .. } => true,
            Command::Comment(args) => !args.is_edit(),
            _ => false,
        }
    }
}

impl Default for Command {
    fn default() -> Self {
        Self::List(ListArgs::default())
    }
}

/// Arguments for the [`Command::List`] subcommand.
#[derive(Parser, Debug)]
pub(crate) struct ListArgs {
    /// Filter for the list of issues that are assigned to '<DID>' (default: me)
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

/// Arguments for the [`Command::Comment`] subcommand.
#[derive(Parser, Debug)]
pub(crate) struct CommentArgs {
    /// ID of the issue
    id: Rev,

    /// The body of the comment
    #[arg(long, short)]
    #[arg(value_name = "MESSAGE")]
    message: Message,

    /// Optionally, the comment to reply to. If not specified, the comment
    /// will be in reply to the issue itself
    #[arg(long, value_name = "COMMENT_ID")]
    #[arg(conflicts_with = "edit")]
    reply_to: Option<Rev>,

    /// Edit a comment by specifying its ID
    #[arg(long, value_name = "COMMENT_ID")]
    #[arg(conflicts_with = "reply_to")]
    edit: Option<Rev>,
}

impl CommentArgs {
    // TODO(finto): this is only needed to avoid announcing edits for the time
    // being
    /// If the comment is editing an existing comment
    pub(crate) fn is_edit(&self) -> bool {
        self.edit.is_some()
    }
}

/// The action that should be performed based on the supplied [`CommentArgs`].
pub(crate) enum CommentAction {
    /// Comment to the main issue thread.
    Comment {
        /// The issue ID
        id: Rev,
        /// The message of the comment.
        message: Message,
    },
    /// Reply to a specific comment in the issue.
    Reply {
        /// The issue ID
        id: Rev,
        /// The message that is being used to reply to the comment.
        message: Message,
        /// The comment ID that is being replied to.
        reply_to: Rev,
    },
    /// Edit a specific comment in the issue.
    Edit {
        /// The issue ID
        id: Rev,
        /// The message that is being used to edit the comment.
        message: Message,
        /// The comment ID that is being edited.
        to_edit: Rev,
    },
}

impl From<CommentArgs> for CommentAction {
    fn from(
        CommentArgs {
            id,
            message,
            reply_to,
            edit,
        }: CommentArgs,
    ) -> Self {
        match (reply_to, edit) {
            (Some(_), Some(_)) => {
                unreachable!("the argument '--reply-to' cannot be used with '--edit'")
            }
            (Some(reply_to), None) => Self::Reply {
                id,
                message,
                reply_to,
            },
            (None, Some(to_edit)) => Self::Edit {
                id,
                message,
                to_edit,
            },
            (None, None) => Self::Comment { id, message },
        }
    }
}

/// Arguments for the [`Command::State`] subcommand.
#[derive(Parser, Debug)]
#[group(id = "state", required = true, multiple = false)]
pub(crate) struct StateArgs {
    /// Change the state to 'open'
    #[arg(long)]
    #[arg(group = "state")]
    pub(crate) open: bool,

    /// Change the state to 'closed'
    #[arg(long)]
    #[arg(group = "state")]
    pub(crate) closed: bool,

    /// Change the state to 'solved'
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
