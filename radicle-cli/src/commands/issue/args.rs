use std::str::FromStr;

use clap::{ArgGroup, Parser, Subcommand, ValueHint};
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
    #[default]
    Me,
    Peer(Did),
}

#[derive(Parser, Debug)]
pub struct Args {
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

    #[arg(long, short)]
    pub(crate) repo: Option<RepoId>,
}

/// Commands to create, view, and edit Radicle issues
#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Delete an issue
    Delete {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,
    },

    /// Edit an issue
    Edit {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        #[arg(long, short)]
        title: Option<String>,

        #[arg(long, short)]
        description: Option<String>,
    },

    /// List issues, optionally filtering them
    List(ListArgs),

    /// Create a new issue
    Open {
        #[arg(long, short)]
        title: Option<String>,

        #[arg(long, short)]
        description: Option<String>,

        #[arg(long)]
        labels: Vec<Label>,

        #[arg(long)]
        assignees: Vec<Did>,
    },

    /// Add a reaction emoji to an issue or comment
    React {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        #[arg(long = "emoji")]
        #[arg(value_name = "char")]
        reaction: Reaction,

        #[arg(long = "to")]
        #[arg(value_name = "comment")]
        // TODO: Add dynamic hint for comment ids
        comment_id: Option<thread::CommentId>,
    },

    /// Manage assignees of an issue
    Assign {
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        #[arg(value_name = "issue-id")]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times).
        #[clap(value_hint = ValueHint::Dynamic(get_did_hints))]
        #[arg(long, short)]
        #[arg(value_name = "did")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Did>,

        /// Delete an assignee from the issue (may be specified multiple times).
        #[clap(value_hint = ValueHint::Dynamic(get_did_hints))]
        #[arg(long, short)]
        #[arg(value_name = "did")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Did>,
    },

    /// Update labels on an issue
    Label {
        /// The issue to label.
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        /// Add an assignee to the issue (may be specified multiple times).
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        add: Vec<Label>,

        /// Delete an assignee from the issue (may be specified multiple times).
        #[arg(long, short)]
        #[arg(value_name = "label")]
        #[arg(action = clap::ArgAction::Append)]
        delete: Vec<Label>,
    },

    /// Add a comment to an issue.
    Comment {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        /// Message text.
        #[arg(long, short)]
        #[arg(value_name = "message")]
        message: Message,

        #[arg(long, name = "comment-id")]
        reply_to: Option<Rev>,
    },

    /// Show a specific issue
    Show {
        #[arg(value_name = "issue-id")]
        #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
        id: Rev,

        /// Show the issue as Rust debug output
        #[arg(long)]
        debug: bool,
    },

    /// Re-cache all issues that can be found in Radicle storage
    Cache {
        #[arg(value_name = "issue-id")]
        id: Option<Rev>,
    },

    /// Set the state of an issue
    State(StateArgs),
}

#[derive(Parser, Debug)]
pub(crate) struct ListArgs {
    /// List issues assigned to <did> (default: me)
    #[clap(value_hint = ValueHint::Dynamic(get_assignee_did_hints))]
    #[arg(long, name = "did")]
    #[arg(default_missing_value = "me")]
    #[arg(num_args = 0..=1)]
    #[arg(require_equals = true)]
    pub(crate) assigned: Option<Assigned>,

    /// List all issues (default)
    #[arg(long, group = "state")]
    all: bool,

    /// List only open issues
    #[arg(long, group = "state")]
    open: bool,

    /// List only closed issues
    #[arg(long, group = "state")]
    closed: bool,

    /// List only solved issues
    #[arg(long, group = "state")]
    solved: bool,
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

#[derive(Parser, Debug)]
#[clap(group(ArgGroup::new("state").required(true)))]
pub(crate) struct StateArgs {
    #[arg(value_name = "issue-id")]
    #[clap(value_hint = ValueHint::Dynamic(get_issue_id_hints))]
    pub(crate) id: Rev,

    /// Set issue state to open
    #[arg(long, short, group = "state")]
    open: bool,

    /// Set issue state to closed
    #[arg(long, short, group = "state")]
    closed: bool,

    /// Set issue state to solved
    #[arg(long, short, group = "state")]
    solved: bool,
}

impl StateArgs {
    pub fn to_state(&self) -> State {
        if self.open {
            State::Open
        } else if self.closed {
            State::Closed {
                reason: CloseReason::Other,
            }
        } else if self.solved {
            State::Closed {
                reason: CloseReason::Solved,
            }
        } else {
            // FIXME:
            unreachable!("State flag needed");
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

pub fn get_assignee_did_hints(input: &str) -> Option<Vec<String>> {
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
                                did.starts_with(input).then_some(did)
                            })
                            .collect::<Vec<_>>()
                    })
                    .ok()
            })
        })
}

pub fn get_issue_id_hints(input: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    let profile = radicle::Profile::load().ok()?;
    let repo = profile.storage.repository(rid).ok()?;
    let issues = profile.issues(&repo).ok()?;
    let ids = issues.ids(input).ok()?;
    Some(
        ids.filter_map(|result| result.ok().map(|id| id.to_string()))
            .collect(),
    )
}

pub fn get_did_hints(input: &str) -> Option<Vec<String>> {
    let (_, rid) = radicle::rad::cwd().ok()?;
    let profile = radicle::Profile::load().ok()?;
    let repo = profile.storage.repository(rid).ok()?;
    let ids = repo.remote_ids().ok()?;
    Some(
        ids.filter_map(|nid| {
            let nid = nid.ok()?;
            let did = Did::from(nid).to_human();
            did.starts_with(input).then_some(did)
        })
        .collect(),
    )
}
