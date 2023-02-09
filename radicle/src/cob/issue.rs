use std::ops::Deref;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use radicle_crdt::clock;
use radicle_crdt::{LWWReg, LWWSet, Max, Semilattice};

use crate::cob;
use crate::cob::common::{Author, Reaction, Tag};
use crate::cob::store::FromHistory as _;
use crate::cob::store::Transaction;
use crate::cob::thread;
use crate::cob::thread::{CommentId, Thread};
use crate::cob::{store, ActorId, ObjectId, OpId, TypeName};
use crate::crypto::{PublicKey, Signer};
use crate::storage::git as storage;

/// Issue operation.
pub type Op = cob::Op<Action>;

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
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::OpError),
    #[error("store: {0}")]
    Store(#[from] store::Error),
}

/// Reason why an issue was closed.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseReason {
    Other,
    Solved,
}

/// Issue state.
#[derive(Debug, Default, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum State {
    /// The issue is closed.
    Closed { reason: CloseReason },
    /// The issue is open.
    #[default]
    Open,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed { .. } => write!(f, "closed"),
            Self::Open { .. } => write!(f, "open"),
        }
    }
}

impl State {
    pub fn lifecycle_message(self) -> String {
        match self {
            State::Open => "Open issue".to_owned(),
            State::Closed { .. } => "Close issue".to_owned(),
        }
    }
}

/// Issue state. Accumulates [`Action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    assignees: LWWSet<ActorId>,
    title: LWWReg<Max<String>, clock::Lamport>,
    state: LWWReg<Max<State>, clock::Lamport>,
    tags: LWWSet<Tag>,
    thread: Thread,
}

impl Semilattice for Issue {
    fn merge(&mut self, other: Self) {
        self.assignees.merge(other.assignees);
        self.title.merge(other.title);
        self.state.merge(other.state);
        self.tags.merge(other.tags);
        self.thread.merge(other.thread);
    }
}

impl Default for Issue {
    fn default() -> Self {
        Self {
            assignees: LWWSet::default(),
            title: Max::from(String::default()).into(),
            state: Max::from(State::default()).into(),
            tags: LWWSet::default(),
            thread: Thread::default(),
        }
    }
}

impl store::FromHistory for Issue {
    type Action = Action;
    type Error = Error;

    fn type_name() -> &'static TypeName {
        &*TYPENAME
    }

    fn apply(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<(), Error> {
        for op in ops {
            match op.action {
                Action::Assign { add, remove } => {
                    for assignee in add {
                        self.assignees.insert(assignee, op.clock);
                    }
                    for assignee in remove {
                        self.assignees.remove(assignee, op.clock);
                    }
                }
                Action::Edit { title } => {
                    self.title.set(title, op.clock);
                }
                Action::Lifecycle { state } => {
                    self.state.set(state, op.clock);
                }
                Action::Tag { add, remove } => {
                    for tag in add {
                        self.tags.insert(tag, op.clock);
                    }
                    for tag in remove {
                        self.tags.remove(tag, op.clock);
                    }
                }
                Action::Thread { action } => {
                    self.thread
                        .apply([cob::Op::new(action, op.author, op.timestamp, op.clock)])?;
                }
            }
        }
        Ok(())
    }
}

impl Issue {
    pub fn assigned(&self) -> impl Iterator<Item = &ActorId> {
        self.assignees.iter()
    }

    pub fn title(&self) -> &str {
        self.title.get().as_str()
    }

    pub fn state(&self) -> &State {
        self.state.get()
    }

    pub fn tags(&self) -> impl Iterator<Item = &Tag> {
        self.tags.iter()
    }

    pub fn author(&self) -> Option<Author> {
        self.thread
            .comments()
            .next()
            .map(|(_, c)| Author::new(c.author()))
    }

    pub fn description(&self) -> Option<&str> {
        self.thread.comments().next().map(|(_, c)| c.body())
    }

    pub fn comments(&self) -> impl Iterator<Item = (&CommentId, &thread::Comment)> {
        self.thread.comments()
    }
}

impl Deref for Issue {
    type Target = Thread;

    fn deref(&self) -> &Self::Target {
        &self.thread
    }
}

impl store::Transaction<Issue> {
    pub fn assign(
        &mut self,
        add: impl IntoIterator<Item = ActorId>,
        remove: impl IntoIterator<Item = ActorId>,
    ) -> OpId {
        let add = add.into_iter().collect::<Vec<_>>();
        let remove = remove.into_iter().collect::<Vec<_>>();

        self.push(Action::Assign { add, remove })
    }

    /// Set the issue title.
    pub fn edit(&mut self, title: impl ToString) -> OpId {
        self.push(Action::Edit {
            title: title.to_string(),
        })
    }

    /// Lifecycle an issue.
    pub fn lifecycle(&mut self, state: State) -> OpId {
        self.push(Action::Lifecycle { state })
    }

    /// Create the issue thread.
    pub fn thread<S: ToString>(&mut self, body: S) -> CommentId {
        self.push(Action::from(thread::Action::Comment {
            body: body.to_string(),
            reply_to: None,
        }))
    }

    /// Comment on an issue.
    pub fn comment<S: ToString>(&mut self, body: S, reply_to: CommentId) -> CommentId {
        self.push(Action::from(thread::Action::Comment {
            body: body.to_string(),
            reply_to: Some(reply_to),
        }))
    }

    /// Tag an issue.
    pub fn tag(
        &mut self,
        add: impl IntoIterator<Item = Tag>,
        remove: impl IntoIterator<Item = Tag>,
    ) -> OpId {
        let add = add.into_iter().collect::<Vec<_>>();
        let remove = remove.into_iter().collect::<Vec<_>>();

        self.push(Action::Tag { add, remove })
    }

    /// React to an issue comment.
    pub fn react(&mut self, to: CommentId, reaction: Reaction) -> OpId {
        self.push(Action::Thread {
            action: thread::Action::React {
                to,
                reaction,
                active: true,
            },
        })
    }
}

pub struct IssueMut<'a, 'g> {
    id: ObjectId,
    clock: clock::Lamport,
    issue: Issue,
    store: &'g mut Issues<'a>,
}

impl<'a, 'g> IssueMut<'a, 'g> {
    /// Get the issue id.
    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Get the internal logical clock.
    pub fn clock(&self) -> &clock::Lamport {
        &self.clock
    }

    /// Assign one or more actors to an issue.
    pub fn assign<G: Signer>(
        &mut self,
        assignees: impl IntoIterator<Item = ActorId>,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("Assign", signer, |tx| tx.assign(assignees, []))
    }

    /// Set the issue title.
    pub fn edit<G: Signer>(&mut self, title: impl ToString, signer: &G) -> Result<OpId, Error> {
        self.transaction("Edit", signer, |tx| tx.edit(title))
    }

    /// Lifecycle an issue.
    pub fn lifecycle<G: Signer>(&mut self, state: State, signer: &G) -> Result<OpId, Error> {
        self.transaction("Lifecycle", signer, |tx| tx.lifecycle(state))
    }

    /// Create the issue thread.
    pub fn thread<G: Signer, S: ToString>(
        &mut self,
        body: S,
        signer: &G,
    ) -> Result<CommentId, Error> {
        self.transaction("Create thread", signer, |tx| tx.thread(body))
    }

    /// Comment on an issue.
    pub fn comment<G: Signer, S: ToString>(
        &mut self,
        body: S,
        reply_to: CommentId,
        signer: &G,
    ) -> Result<CommentId, Error> {
        assert!(self.thread.comment(&reply_to).is_some());
        self.transaction("Comment", signer, |tx| tx.comment(body, reply_to))
    }

    /// Tag an issue.
    pub fn tag<G: Signer>(
        &mut self,
        add: impl IntoIterator<Item = Tag>,
        remove: impl IntoIterator<Item = Tag>,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("Tag", signer, |tx| tx.tag(add, remove))
    }

    /// React to an issue comment.
    pub fn react<G: Signer>(
        &mut self,
        to: CommentId,
        reaction: Reaction,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("React", signer, |tx| tx.react(to, reaction))
    }

    /// Unassign one or more actors from an issue.
    pub fn unassign<G: Signer>(
        &mut self,
        assignees: impl IntoIterator<Item = ActorId>,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("Unassign", signer, |tx| tx.assign([], assignees))
    }

    pub fn transaction<G, F, T>(
        &mut self,
        message: &str,
        signer: &G,
        operations: F,
    ) -> Result<T, Error>
    where
        G: Signer,
        F: FnOnce(&mut Transaction<Issue>) -> T,
    {
        let mut tx = Transaction::new(*signer.public_key(), self.clock);
        let output = operations(&mut tx);
        let (ops, clock) = tx.commit(message, self.id, &mut self.store.raw, signer)?;

        self.issue.apply(ops)?;
        self.clock = clock;

        Ok(output)
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
        title: impl ToString,
        description: impl ToString,
        tags: &[Tag],
        assignees: &[ActorId],
        signer: &G,
    ) -> Result<IssueMut<'a, 'g>, Error> {
        let (id, issue, clock) =
            Transaction::initial("Create issue", &mut self.raw, signer, |tx| {
                tx.thread(description);
                tx.assign(assignees.to_owned(), []);
                tx.edit(title);
                tx.tag(tags.to_owned(), []);
            })?;
        // Just a sanity check that our clock is advancing as expected.
        debug_assert_eq!(clock.get(), 4);

        Ok(IssueMut {
            id,
            clock,
            issue,
            store: self,
        })
    }

    /// Remove an issue.
    pub fn remove(&self, id: &ObjectId) -> Result<(), store::Error> {
        self.raw.remove(id)
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
        state: State,
    },
    Tag {
        add: Vec<Tag>,
        remove: Vec<Tag>,
    },
    Thread {
        action: thread::Action,
    },
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
    use crate::test::arbitrary;

    #[test]
    fn test_ordering() {
        assert!(CloseReason::Solved > CloseReason::Other);
        assert!(
            State::Open
                > State::Closed {
                    reason: CloseReason::Solved
                }
        );
    }

    #[test]
    fn test_issue_create_and_assign() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();

        let assignee: ActorId = arbitrary::gen(1);
        let assignee_two: ActorId = arbitrary::gen(1);
        let issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[assignee],
                &signer,
            )
            .unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let assignees: Vec<_> = issue.assigned().cloned().collect::<Vec<_>>();

        assert_eq!(1, assignees.len());
        assert!(assignees.contains(&assignee));

        let mut issue = issues.get_mut(&id).unwrap();
        issue.assign([assignee_two], &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let assignees: Vec<_> = issue.assigned().cloned().collect::<Vec<_>>();

        assert_eq!(2, assignees.len());
        assert!(assignees.contains(&assignee));
        assert!(assignees.contains(&assignee_two));
    }

    #[test]
    fn test_issue_create_and_reassign() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();

        let assignee: ActorId = arbitrary::gen(1);
        let assignee_two: ActorId = arbitrary::gen(1);
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[assignee],
                &signer,
            )
            .unwrap();

        issue.assign([assignee_two], &signer).unwrap();
        issue.assign([assignee_two], &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let assignees: Vec<_> = issue.assigned().cloned().collect::<Vec<_>>();

        assert_eq!(2, assignees.len());
        assert!(assignees.contains(&assignee));
        assert!(assignees.contains(&assignee_two));
    }

    #[test]
    fn test_issue_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let created = issues
            .create("My first issue", "Blah blah blah.", &[], &[], &signer)
            .unwrap();

        assert_eq!(created.clock().get(), 4);

        let (id, created) = (created.id, created.issue);
        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(created, issue);
        assert_eq!(issue.title(), "My first issue");
        assert_eq!(issue.author(), Some(issues.author()));
        assert_eq!(issue.description(), Some("Blah blah blah."));
        assert_eq!(issue.comments().count(), 1);
        assert_eq!(issue.state(), &State::Open);
    }

    #[test]
    fn test_issue_create_and_change_state() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &[], &signer)
            .unwrap();

        issue
            .lifecycle(
                State::Closed {
                    reason: CloseReason::Other,
                },
                &signer,
            )
            .unwrap();

        let id = issue.id;
        let mut issue = issues.get_mut(&id).unwrap();

        assert_eq!(
            *issue.state(),
            State::Closed {
                reason: CloseReason::Other
            }
        );

        issue.lifecycle(State::Open, &signer).unwrap();
        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(*issue.state(), State::Open);
    }

    #[test]
    fn test_issue_create_and_unassign() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();

        let assignee: ActorId = arbitrary::gen(1);
        let assignee_two: ActorId = arbitrary::gen(1);
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[],
                &[assignee, assignee_two],
                &signer,
            )
            .unwrap();

        issue.unassign([assignee], &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let assignees: Vec<_> = issue.assigned().cloned().collect::<Vec<_>>();

        assert_eq!(1, assignees.len());
        assert!(assignees.contains(&assignee_two));
    }

    #[test]
    fn test_issue_edit_title() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &[], &signer)
            .unwrap();

        issue.edit("Sorry typo", &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let r = issue.title();

        assert_eq!(r, "Sorry typo");
    }

    #[test]
    fn test_issue_react() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &[], &signer)
            .unwrap();

        let comment = OpId::initial(*signer.public_key());
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
        let author = *signer.public_key();
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let mut issue = issues
            .create("My first issue", "Blah blah blah.", &[], &[], &signer)
            .unwrap();
        let root = OpId::root(author);

        let c1 = issue.comment("Hi hi hi.", root, &signer).unwrap();
        let c2 = issue.comment("Ha ha ha.", root, &signer).unwrap();

        let id = issue.id;
        let mut issue = issues.get_mut(&id).unwrap();
        let (_, reply1) = &issue.replies(&root).nth(0).unwrap();
        let (_, reply2) = &issue.replies(&root).nth(1).unwrap();

        assert_eq!(reply1.body(), "Hi hi hi.");
        assert_eq!(reply2.body(), "Ha ha ha.");

        issue.comment("Re: Hi.", c1, &signer).unwrap();
        issue.comment("Re: Ha.", c2, &signer).unwrap();
        issue.comment("Re: Ha. Ha.", c2, &signer).unwrap();
        issue.comment("Re: Ha. Ha. Ha.", c2, &signer).unwrap();

        let issue = issues.get(&id).unwrap().unwrap();

        assert_eq!(issue.replies(&c1).nth(0).unwrap().1.body(), "Re: Hi.");
        assert_eq!(issue.replies(&c2).nth(0).unwrap().1.body(), "Re: Ha.");
        assert_eq!(issue.replies(&c2).nth(1).unwrap().1.body(), "Re: Ha. Ha.");
        assert_eq!(
            issue.replies(&c2).nth(2).unwrap().1.body(),
            "Re: Ha. Ha. Ha."
        );
    }

    #[test]
    fn test_issue_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut issues = Issues::open(*signer.public_key(), &project).unwrap();
        let bug_tag = Tag::new("bug").unwrap();
        let ux_tag = Tag::new("ux").unwrap();
        let wontfix_tag = Tag::new("wontfix").unwrap();
        let mut issue = issues
            .create(
                "My first issue",
                "Blah blah blah.",
                &[ux_tag.clone()],
                &[],
                &signer,
            )
            .unwrap();

        issue.tag([bug_tag.clone()], [], &signer).unwrap();
        issue.tag([wontfix_tag.clone()], [], &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let tags = issue.tags().cloned().collect::<Vec<_>>();

        assert!(tags.contains(&ux_tag));
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
            .create("My first issue", "Blah blah blah.", &[], &[], &signer)
            .unwrap();

        // The root thread op id is always the same.
        let c0 = OpId::root(author);

        issue.comment("Ho ho ho.", c0, &signer).unwrap();
        issue.comment("Ha ha ha.", c0, &signer).unwrap();

        let id = issue.id;
        let issue = issues.get(&id).unwrap().unwrap();
        let (_, c0) = &issue.comments().nth(0).unwrap();
        let (_, c1) = &issue.comments().nth(1).unwrap();
        let (_, c2) = &issue.comments().nth(2).unwrap();

        assert_eq!(c0.body(), "Blah blah blah.");
        assert_eq!(c0.author(), author);
        assert_eq!(c1.body(), "Ho ho ho.");
        assert_eq!(c1.author(), author);
        assert_eq!(c2.body(), "Ha ha ha.");
        assert_eq!(c2.author(), author);
    }

    #[test]
    fn test_issue_state_serde() {
        assert_eq!(
            serde_json::to_value(State::Open).unwrap(),
            serde_json::json!({ "status": "open" })
        );

        assert_eq!(
            serde_json::to_value(State::Closed {
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

        issues.create("First", "Blah", &[], &[], &signer).unwrap();
        issues.create("Second", "Blah", &[], &[], &signer).unwrap();
        issues.create("Third", "Blah", &[], &[], &signer).unwrap();

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
