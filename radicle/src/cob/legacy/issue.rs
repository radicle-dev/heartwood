use serde::{Deserialize, Serialize};

use crate::cob;
use crate::cob::common::Label;
use crate::cob::issue;
use crate::cob::store::HistoryAction;
use crate::cob::thread;
use crate::cob::{store, ActorId, TypeName};
use crate::prelude::ReadRepository;

/// Issue operation.
pub type Op = cob::Op<Action>;
/// Error type.
pub type Error = issue::Error;

/// Issue state. Accumulates [`Action`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Issue(issue::Issue);

impl From<Issue> for issue::Issue {
    fn from(issue: Issue) -> issue::Issue {
        issue.0
    }
}

impl store::FromHistory for Issue {
    type Action = Action;
    type Error = Error;

    fn type_name() -> &'static TypeName {
        &*issue::TYPENAME
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.0.title.is_empty() {
            return Err(Error::Validate("title is empty"));
        }
        if self.0.thread.validate().is_err() {
            return Err(Error::Validate("invalid thread"));
        }
        Ok(())
    }

    fn apply<R: ReadRepository>(&mut self, op: Op, repo: &R) -> Result<(), Error> {
        let issue = &mut self.0;

        for action in op.actions {
            match action {
                Action::Assign { add, remove } => {
                    for assignee in add {
                        issue.assignees.insert(assignee.into());
                    }
                    for assignee in remove {
                        issue.assignees.remove(&assignee.into());
                    }
                }
                Action::Edit { title } => {
                    issue.title = title;
                }
                Action::Lifecycle { state } => {
                    issue.state = state;
                }
                Action::Tag { add, remove } => {
                    for tag in add {
                        issue.labels.insert(tag);
                    }
                    for tag in remove {
                        issue.labels.remove(&tag);
                    }
                }
                Action::Thread { action } => {
                    issue.thread.apply(
                        cob::Op::new(
                            op.id,
                            action,
                            op.author,
                            op.timestamp,
                            op.identity,
                            op.manifest.clone(),
                        ),
                        repo,
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Issue operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Assign {
        add: Vec<ActorId>,
        remove: Vec<ActorId>,
    },
    Edit {
        title: String,
    },
    Lifecycle {
        state: issue::State,
    },
    Tag {
        add: Vec<Label>,
        remove: Vec<Label>,
    },
    Thread {
        action: thread::Action,
    },
}

impl HistoryAction for Action {}

impl From<thread::Action> for Action {
    fn from(action: thread::Action) -> Self {
        Self::Thread { action }
    }
}
