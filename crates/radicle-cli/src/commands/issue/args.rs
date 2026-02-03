use std::str::FromStr;

use clap::{Parser, Subcommand};

use radicle::{
    cob::{Label, Reaction, Title},
    identity::{did::DidError, Did, RepoId},
    issue::{CloseReason, State},
};

use crate::{git::Rev, terminal::patch::Message};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum Assigned {
    #[default]
    Me,
    Peer(Did),
}

#[derive(Parser, Debug)]
#[command(about = super::ABOUT, disable_version_flag = true)]
pub struct Args {
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

    /// Arguments for the empty subcommand.
    /// Will fall back to [`Command::List`].
    #[clap(flatten)]
    pub(crate) empty: EmptyArgs,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Add or delete assignees from an issue
    Assign {
        /// ID of the issue
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// Add an assignee (may be specified multiple times, takes precedence over `--delete`)
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
    #[clap(long_about = include_str!("comment.txt"))]
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
        #[arg(value_name = "ISSUE_ID")]
        id: Rev,

        /// Add a label (may be specified multiple times, takes precedence over `--delete`)
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Label>,

        /// Delete a label (may be specified multiple times)
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
        #[arg(value_name = "ISSUE_ID")]
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
            Command::Cache{..} | Command::Show { .. } | Command::List(_) => false,
        }
    }
}

/// Arguments for the empty subcommand.
#[derive(Parser, Debug, Default)]
pub(crate) struct EmptyArgs {
    #[arg(long, name = "DID")]
    #[arg(default_missing_value = "me")]
    #[arg(num_args = 0..=1)]
    #[arg(hide = true)]
    pub(crate) assigned: Option<Assigned>,

    #[clap(flatten)]
    pub(crate) state: EmptyStateArgs,
}

/// Counterpart to [`ListStateArgs`] for the empty subcommand.
#[derive(Parser, Debug, Default)]
#[group(multiple = false)]
pub(crate) struct EmptyStateArgs {
    #[arg(long, hide = true)]
    all: bool,

    #[arg(long, hide = true)]
    open: bool,

    #[arg(long, hide = true)]
    closed: bool,

    #[arg(long, hide = true)]
    solved: bool,
}

/// Arguments for the [`Command::List`] subcommand.
#[derive(Parser, Debug, Default)]
pub(crate) struct ListArgs {
    /// Filter for the list of issues that are assigned to '<DID>' (default: me)
    #[arg(long, name = "DID")]
    #[arg(default_missing_value = "me")]
    #[arg(num_args = 0..=1)]
    pub(crate) assigned: Option<Assigned>,

    #[clap(flatten)]
    pub(crate) state: ListStateArgs,
}

#[derive(Parser, Debug, Default)]
#[group(multiple = false)]
pub(crate) struct ListStateArgs {
    /// List all issues
    #[arg(long)]
    all: bool,

    /// List only open issues (default)
    #[arg(long)]
    open: bool,

    /// List only closed issues
    #[arg(long)]
    closed: bool,

    /// List only solved issues
    #[arg(long)]
    solved: bool,
}

impl From<&ListStateArgs> for Option<State> {
    fn from(args: &ListStateArgs) -> Self {
        match (args.all, args.open, args.closed, args.solved) {
            (true, false, false, false) => None,
            (false, true, false, false) | (false, false, false, false) => Some(State::Open),
            (false, false, true, false) => Some(State::Closed {
                reason: CloseReason::Other,
            }),
            (false, false, false, true) => Some(State::Closed {
                reason: CloseReason::Solved,
            }),
            _ => unreachable!(),
        }
    }
}

impl From<EmptyStateArgs> for ListStateArgs {
    fn from(args: EmptyStateArgs) -> Self {
        Self {
            all: args.all,
            open: args.open,
            closed: args.closed,
            solved: args.solved,
        }
    }
}

impl From<EmptyArgs> for ListArgs {
    fn from(args: EmptyArgs) -> Self {
        Self {
            assigned: args.assigned,
            state: ListStateArgs::from(args.state),
        }
    }
}

/// Arguments for the [`Command::Comment`] subcommand.
#[derive(Parser, Debug)]
pub(crate) struct CommentArgs {
    /// ID of the issue
    #[arg(value_name = "ISSUE_ID")]
    id: Rev,

    /// The body of the comment
    #[arg(long, short)]
    #[arg(value_name = "MESSAGE")]
    message: Option<Message>,

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

/// Arguments for the [`Command::State`] subcommand.
#[derive(Parser, Debug)]
#[group(required = true, multiple = false)]
pub(crate) struct StateArgs {
    /// Change the state to 'open'
    #[arg(long)]
    pub(crate) open: bool,

    /// Change the state to 'closed'
    #[arg(long)]
    pub(crate) closed: bool,

    /// Change the state to 'solved'
    #[arg(long)]
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

/// The action that should be performed based on the supplied [`CommentArgs`].
pub(crate) enum CommentAction {
    /// Comment to the main issue thread.
    Comment {
        /// ID of the issue
        id: Rev,
        /// The message of the comment.
        message: Message,
    },
    /// Reply to a specific comment in the issue.
    Reply {
        /// ID of the issue
        id: Rev,
        /// The message that is being used to reply to the comment.
        message: Message,
        /// The comment ID that is being replied to.
        reply_to: Rev,
    },
    /// Edit a specific comment in the issue.
    Edit {
        /// ID of the issue
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
        let message = message.unwrap_or(Message::Edit);
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
