use std::ops::{ControlFlow, Deref};
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt as crdt;
use radicle_crdt::clock;
use radicle_crdt::{ChangeId, LWWReg, Max, Semilattice};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob::common::{Author, Reaction, Tag};
use crate::cob::thread;
use crate::cob::thread::{CommentId, Thread};
use crate::cob::{store, ObjectId, TypeName};
use crate::crypto::{PublicKey, Signer};
use crate::storage::git as storage;

pub type Change = crdt::Change<Action>;

/// Type name of an issue.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.issue").expect("type name is valid"));

/// Identifier for an issue.
pub type IssueId = ObjectId;

/// Error updating or creating issues.
#[derive(Error, Debug)]
pub enum Error {
    #[error("apply failed")]
    Apply,
    #[error("store: {0}")]
    Store(#[from] store::Error),
}

/// Reason why an issue was closed.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseReason {
    Other,
    Solved,
}

/// Issue state.
#[derive(Debug, Default, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "status")]
pub enum Status {
    /// The issue is closed.
    Closed { reason: CloseReason },
    /// The issue is open.
    #[default]
    Open,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed { .. } => write!(f, "closed"),
            Self::Open { .. } => write!(f, "open"),
        }
    }
}

impl Status {
    pub fn lifecycle_message(self) -> String {
        match self {
            Status::Open => "Open issue".to_owned(),
            Status::Closed { .. } => "Close issue".to_owned(),
        }
    }
}

/// Issue state. Accumulates [`Action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    // TODO(cloudhead): Title should bias towards shorter strings.
    title: LWWReg<Max<String>, clock::Lamport>,
    status: LWWReg<Max<Status>, clock::Lamport>,
    thread: Thread,
}

impl Semilattice for Issue {
    fn merge(&mut self, other: Self) {
        self.title.merge(other.title);
        self.status.merge(other.status);
        self.thread.merge(other.thread);
    }
}

impl Default for Issue {
    fn default() -> Self {
        Self {
            title: Max::from(String::default()).into(),
            status: Max::from(Status::default()).into(),
            thread: Thread::default(),
        }
    }
}

impl store::FromHistory for Issue {
    type Action = Action;

    fn type_name() -> &'static TypeName {
        &*TYPENAME
    }

    fn from_history(
        history: &radicle_cob::History,
    ) -> Result<(Self, clock::Lamport), store::Error> {
        let obj = history.traverse(Self::default(), |mut acc, entry| {
            if let Ok(action) = Action::decode(entry.contents()) {
                if let Err(err) = acc.apply(Change {
                    action,
                    author: *entry.actor(),
                    clock: entry.clock().into(),
                }) {
                    log::warn!("Error applying change to issue state: {err}");
                    return ControlFlow::Break(acc);
                }
            } else {
                return ControlFlow::Break(acc);
            }
            ControlFlow::Continue(acc)
        });

        Ok((obj, history.clock().into()))
    }
}

impl Issue {
    pub fn title(&self) -> &str {
        self.title.get().as_str()
    }

    pub fn status(&self) -> &Status {
        self.status.get()
    }

    pub fn tags(&self) -> impl Iterator<Item = &Tag> {
        self.thread.tags()
    }

    pub fn author(&self) -> Option<Author> {
        self.thread
            .comments()
            .next()
            .map(|((_, pk), _)| Author::new(*pk))
    }

    pub fn description(&self) -> Option<&str> {
        self.thread.comments().next().map(|(_, c)| c.body.as_str())
    }

    pub fn comments(&self) -> impl Iterator<Item = (&CommentId, &thread::Comment)> {
        self.thread.comments().map(|(id, comment)| (id, comment))
    }

    pub fn apply(&mut self, change: Change) -> Result<(), Error> {
        match change.action {
            Action::Title { title } => {
                self.title.set(title, change.clock);
            }
            Action::Lifecycle { status } => {
                self.status.set(status, change.clock);
            }
            Action::Thread { action } => {
                self.thread.apply([crdt::Change {
                    action,
                    author: change.author,
                    clock: change.clock,
                }]);
            }
        }
        Ok(())
    }
}

impl Deref for Issue {
    type Target = Thread;

    fn deref(&self) -> &Self::Target {
        &self.thread
    }
}

pub struct IssueMut<'a, 'g> {
    id: ObjectId,
    clock: clock::Lamport,
    issue: Issue,
    store: &'g mut Issues<'a>,
}

impl<'a, 'g> IssueMut<'a, 'g> {
    /// Get the internal logical clock.
    pub fn clock(&self) -> &clock::Lamport {
        &self.clock
    }

    /// Lifecycle an issue.
    pub fn lifecycle<G: Signer>(&mut self, status: Status, signer: &G) -> Result<ChangeId, Error> {
        let action = Action::Lifecycle { status };
        self.apply("Lifecycle", action, signer)
    }

    /// Comment on an issue.
    pub fn comment<G: Signer, S: Into<String>>(
        &mut self,
        body: S,
        signer: &G,
    ) -> Result<CommentId, Error> {
        let body = body.into();
        let action = Action::from(thread::Action::Comment {
            body,
            reply_to: None,
        });
        self.apply("Comment", action, signer)
    }

    /// Tag an issue.
    pub fn tag<G: Signer>(
        &mut self,
        tags: impl IntoIterator<Item = Tag>,
        signer: &G,
    ) -> Result<ChangeId, Error> {
        let tags = tags.into_iter().collect::<Vec<_>>();
        let action = Action::Thread {
            action: thread::Action::Tag { tags },
        };
        self.apply("Tag", action, signer)
    }

    /// Reply to on an issue comment.
    pub fn reply<G: Signer, S: Into<String>>(
        &mut self,
        parent: CommentId,
        body: S,
        signer: &G,
    ) -> Result<ChangeId, Error> {
        let body = body.into();

        assert!(self.thread.comment(&parent).is_some());

        let action = Action::from(thread::Action::Comment {
            body,
            reply_to: Some(parent),
        });
        self.apply("Reply", action, signer)
    }

    /// React to an issue comment.
    pub fn react<G: Signer>(
        &mut self,
        to: CommentId,
        reaction: Reaction,
        signer: &G,
    ) -> Result<ChangeId, Error> {
        let action = Action::Thread {
            action: thread::Action::React {
                to,
                reaction,
                active: true,
            },
        };
        self.apply("React", action, signer)
    }

    /// Apply a change to the issue.
    pub fn apply<G: Signer>(
        &mut self,
        msg: &'static str,
        action: Action,
        signer: &G,
    ) -> Result<ChangeId, Error> {
        let cob = self
            .store
            .update(self.id, msg, action.clone(), signer)
            .map_err(Error::Store)?;
        let clock = cob.history().clock();

        let change = Change {
            author: *signer.public_key(),
            action,
            clock: clock.into(),
        };
        self.issue.apply(change)?;

        Ok((clock.into(), *signer.public_key()))
    }
}

impl<'a, 'g> Deref for IssueMut<'a, 'g> {
    type Target = Issue;

    fn deref(&self) -> &Self::Target {
        &self.issue
    }
}

pub struct Issues<'a> {
    raw: store::Store<'a, Issue>,
}

impl<'a> Deref for Issues<'a> {
    type Target = store::Store<'a, Issue>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a> Issues<'a> {
    /// Open an issues store.
    pub fn open(
        whoami: PublicKey,
        repository: &'a storage::Repository,
    ) -> Result<Self, store::Error> {
        let raw = store::Store::open(whoami, repository)?;

        Ok(Self { raw })
    }

    /// Get an issue.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Issue>, store::Error> {
        self.raw.get(id).map(|r| r.map(|(i, _clock)| i))
    }

    /// Get an issue mutably.
    pub fn get_mut<'g>(&'g mut self, id: &ObjectId) -> Result<IssueMut<'a, 'g>, store::Error> {
        let (issue, clock) = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(IssueMut {
            id: *id,
            clock,
            issue,
            store: self,
        })
    }

    /// Create a new issue.
    pub fn create<'g, G: Signer>(
        &'g mut self,
        title: impl Into<String>,
        description: impl Into<String>,
        tags: &[Tag],
        signer: &G,
    ) -> Result<IssueMut<'a, 'g>, Error> {
        let title = title.into();
        let description = description.into();
        let action = Action::Title { title };
        let (id, issue, clock) = self.raw.create("Create issue", action, signer)?;
        let mut issue = IssueMut {
            id,
            clock,
            issue,
            store: self,
        };

        issue.comment(description, signer)?;
        issue.tag(tags.to_owned(), signer)?;

        Ok(issue)
    }

    /// Remove an issue.
    pub fn remove(&self, id: &ObjectId) -> Result<(), store::Error> {
        self.raw.remove(id)
    }
}

/// Issue operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Action {
    Title { title: String },
    Lifecycle { status: Status },
    Thread { action: thread::Action },
}

impl Action {
    pub fn decode(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl From<thread::Action> for Action {
    fn from(action: thread::Action) -> Self {
        Self::Thread { action }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cob::Reaction;
    use crate::test;

    #[test]
    fn test_ordering() {
        assert!(CloseReason::Solved > CloseReason::Other);
        assert!(
            Status::Open
                > Status::Closed {
                    reason: CloseReason::Solved
                }
        );
    }

    #[test]
    fn test_issue_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let created = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();
        let (id, created) = (created.id, created.issue);
        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(created, issue);
        assert_eq!(issue.title(), "My first issue");
        assert_eq!(issue.author(), Some(issues.author()));
        assert_eq!(issue.description(), Some("Blah blah blah."));
        assert_eq!(issue.comments().count(), 1);
        assert_eq!(issue.status(), &Status::Open);
    }

    #[test]
    fn test_issue_create_and_change_state() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        issue
            .lifecycle(
                Status::Closed {
                    reason: CloseReason::Other,
                },
                &signer,
            )
            .unwrap();

        let id = issue.id;
        let mut issue = issues.get_mut(&id).unwrap();
        assert_eq!(
            *issue.status(),
            Status::Closed {
                reason: CloseReason::Other
            }
        );

        issue.lifecycle(Status::Open, &signer).unwrap();
        let issue = issues.get(&id).unwrap().unwrap();
        assert_eq!(*issue.status(), Status::Open);
    }

    #[test]
    fn test_issue_react() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        let comment = (clock::Lamport::default(), *signer.public_key());
        let reaction = Reaction::new('🥳').unwrap();
        issue.react(comment, reaction, &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let (_, r) = issue.reactions(&comment).next().unwrap();

        assert_eq!(r, &reaction);

        // TODO: Test multiple reactions from same author and different authors
    }

    #[test]
    fn test_issue_reply() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();
        let comment = issue.comment("Ho ho ho.", &signer).unwrap();

        issue.reply(comment, "Hi hi hi.", &signer).unwrap();
        issue.reply(comment, "Ha ha ha.", &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let (_, reply1) = &issue.replies(&comment).nth(0).unwrap();
        let (_, reply2) = &issue.replies(&comment).nth(1).unwrap();

        assert_eq!(reply1.body, "Hi hi hi.");
        assert_eq!(reply2.body, "Ha ha ha.");
    }

    #[test]
    fn test_issue_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        let bug_tag = Tag::new("bug").unwrap();
        let wontfix_tag = Tag::new("wontfix").unwrap();

        issue.tag([bug_tag.clone()], &signer).unwrap();
        issue.tag([wontfix_tag.clone()], &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let tags = issue.tags().cloned().collect::<Vec<_>>();

        assert!(tags.contains(&bug_tag));
        assert!(tags.contains(&wontfix_tag));
    }

    #[test]
    fn test_issue_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let author = *signer.public_key();
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        issue.comment("Ho ho ho.", &signer).unwrap();
        issue.comment("Ha ha ha.", &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let ((_, a0), c0) = &issue.comments().nth(0).unwrap();
        let ((_, a1), c1) = &issue.comments().nth(1).unwrap();
        let ((_, a2), c2) = &issue.comments().nth(2).unwrap();

        assert_eq!(&c0.body, "Blah blah blah.");
        assert_eq!(a0, &author);
        assert_eq!(&c1.body, "Ho ho ho.");
        assert_eq!(a1, &author);
        assert_eq!(&c2.body, "Ha ha ha.");
        assert_eq!(a2, &author);
    }

    #[test]
    fn test_issue_state_serde() {
        assert_eq!(
            serde_json::to_value(Status::Open).unwrap(),
            serde_json::json!({ "status": "open" })
        );

        assert_eq!(
            serde_json::to_value(Status::Closed {
                reason: CloseReason::Solved
            })
            .unwrap(),
            serde_json::json!({ "status": "closed", "reason": "solved" })
        );
    }

    #[test]
    fn test_issue_all() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();

        issues.create("First", "Blah", &[], &signer).unwrap();
        issues.create("Second", "Blah", &[], &signer).unwrap();
        issues.create("Third", "Blah", &[], &signer).unwrap();

        let issues = issues
            .all()
            .unwrap()
            .map(|r| r.map(|(_, i, _)| i))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(issues.len(), 3);

        issues.iter().find(|i| i.title() == "First").unwrap();
        issues.iter().find(|i| i.title() == "Second").unwrap();
        issues.iter().find(|i| i.title() == "Third").unwrap();
    }
}
