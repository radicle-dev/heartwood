use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::cob::common::*;
use crate::cob::{ObjectId, Timestamp, TypeName};

/// Type name of an issue.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.issue").expect("type name is valid"));

/// Identifier for an issue.
pub type IssueId = ObjectId;

/// Reason why an issue was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseReason {
    Solved,
    Other,
}

/// Issue state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "status")]
pub enum State {
    /// The issue is open.
    Open,
    /// The issue is closed.
    Closed { reason: CloseReason },
}

impl State {
    pub fn lifecycle_message(self) -> String {
        match self {
            State::Open => "Open issue".to_owned(),
            State::Closed { .. } => "Close issue".to_owned(),
        }
    }
}

/// An issue or "ticket".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub author: Author,
    pub title: String,
    pub state: State,
    pub comment: Comment,
    pub discussion: Discussion,
    pub labels: HashSet<Label>,
    pub timestamp: Timestamp,
}

impl Issue {
    pub fn author(&self) -> &Author {
        &self.author
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn description(&self) -> &str {
        &self.comment.body
    }

    pub fn reactions(&self) -> &HashMap<Reaction, usize> {
        &self.comment.reactions
    }

    pub fn comments(&self) -> &[Comment<Replies>] {
        &self.discussion
    }

    pub fn labels(&self) -> &HashSet<Label> {
        &self.labels
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}
