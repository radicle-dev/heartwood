#![allow(clippy::large_enum_variant)]
use std::collections::HashSet;
use std::convert::TryFrom;
use std::ops::ControlFlow;

use automerge::{Automerge, ObjType, ScalarValue, Value};

use crate::cob::automerge::doc::{Document, DocumentError};
use crate::cob::automerge::shared;
use crate::cob::automerge::shared::*;
use crate::cob::automerge::store::{Error, Store};
use crate::cob::automerge::transaction::{Transaction, TransactionError};
use crate::cob::automerge::value::{FromValue, ValueError};
use crate::cob::common::*;
use crate::cob::issue::*;
use crate::cob::{Contents, History, ObjectId, Timestamp, TypeName};
use crate::prelude::*;

impl From<State> for ScalarValue {
    fn from(state: State) -> Self {
        match state {
            State::Open => ScalarValue::from("open"),
            State::Closed {
                reason: CloseReason::Solved,
            } => ScalarValue::from("solved"),
            State::Closed {
                reason: CloseReason::Other,
            } => ScalarValue::from("closed"),
        }
    }
}

impl<'a> FromValue<'a> for State {
    fn from_value(value: Value) -> Result<Self, ValueError> {
        let state = value.to_str().ok_or(ValueError::InvalidType)?;

        match state {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed {
                reason: CloseReason::Other,
            }),
            "solved" => Ok(Self::Closed {
                reason: CloseReason::Solved,
            }),
            _ => Err(ValueError::InvalidValue(value.to_string())),
        }
    }
}

impl FromHistory for Issue {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }

    fn from_history(history: &History) -> Result<Self, Error> {
        let doc = history.traverse(Automerge::new(), |mut doc, entry| {
            let bytes = entry.contents();
            match automerge::Change::from_bytes(bytes.clone()) {
                Ok(change) => {
                    doc.apply_changes([change]).ok();
                }
                Err(_err) => {
                    // Ignore
                }
            }
            ControlFlow::Continue(doc)
        });
        let issue = Issue::try_from(doc)?;

        Ok(issue)
    }
}

impl TryFrom<&History> for Issue {
    type Error = Error;

    fn try_from(history: &History) -> Result<Self, Self::Error> {
        Issue::from_history(history)
    }
}

impl TryFrom<Automerge> for Issue {
    type Error = DocumentError;

    fn try_from(doc: Automerge) -> Result<Self, Self::Error> {
        let doc = Document::new(&doc);
        let obj_id = doc.get_id(automerge::ObjId::Root, "issue")?;
        let title = doc.get(&obj_id, "title")?;
        let comment_id = doc.get_id(&obj_id, "comment")?;
        let author = doc.get(&obj_id, "author").map(Author::new)?;
        let state = doc.get(&obj_id, "state")?;
        let timestamp = doc.get(&obj_id, "timestamp")?;

        let comment = shared::lookup::comment(doc, &comment_id)?;
        let discussion: Discussion = doc.list(&obj_id, "discussion", shared::lookup::thread)?;
        let labels: HashSet<Label> = doc.keys(&obj_id, "labels")?;

        Ok(Self {
            title,
            state,
            author,
            comment,
            discussion,
            labels,
            timestamp,
        })
    }
}

pub struct IssueStore<'a> {
    store: Store<'a, Issue>,
}

impl<'a> IssueStore<'a> {
    /// Create a new issue store.
    pub fn new(store: Store<'a, Issue>) -> Self {
        Self { store }
    }

    /// Get an issue by id.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Issue>, Error> {
        self.store.get(id)
    }

    /// Create an issue.
    pub fn create<G: Signer>(
        &self,
        title: &str,
        description: &str,
        labels: &[Label],
        signer: &G,
    ) -> Result<IssueId, Error> {
        let author = self.store.author();
        let timestamp = Timestamp::now();
        let contents = events::create(&author, title, description, timestamp, labels)?;
        let cob = self.store.create("Create issue", contents, signer)?;

        Ok(*cob.id())
    }

    /// Remove an issue.
    pub fn remove<G: Signer>(&self, _issue_id: &IssueId, _signer: &G) -> Result<(), Error> {
        todo!()
    }

    /// Comment on an issue.
    pub fn comment<G: Signer>(
        &self,
        issue_id: &IssueId,
        body: &str,
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.store.author();
        let mut issue = self.store.get_raw(issue_id)?;
        let timestamp = Timestamp::now();
        let changes = events::comment(&mut issue, &author, body, timestamp)?;

        self.store
            .update(*issue_id, "Add comment", changes, signer)?;

        Ok(())
    }

    /// Life-cycle an issue, eg. open or close it.
    pub fn lifecycle<G: Signer>(
        &self,
        issue_id: &IssueId,
        state: State,
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.store.author();
        let mut issue = self.store.get_raw(issue_id)?;
        let changes = events::lifecycle(&mut issue, &author, state)?;

        self.store.update(*issue_id, "Lifecycle", changes, signer)?;

        Ok(())
    }

    /// Label an issue.
    pub fn label<G: Signer>(
        &self,
        issue_id: &IssueId,
        labels: &[Label],
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.store.author();
        let mut issue = self.store.get_raw(issue_id)?;
        let changes = events::label(&mut issue, &author, labels)?;

        self.store.update(*issue_id, "Add label", changes, signer)?;

        Ok(())
    }

    /// React to an issue comment.
    pub fn react<G: Signer>(
        &self,
        issue_id: &IssueId,
        comment_id: CommentId,
        reaction: Reaction,
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.store.author();
        let mut issue = self.store.get_raw(issue_id)?;
        let changes = events::react(&mut issue, comment_id, &author, &[reaction])?;

        self.store.update(*issue_id, "React", changes, signer)?;

        Ok(())
    }

    /// Reply to an issue comment.
    pub fn reply<G: Signer>(
        &self,
        issue_id: &IssueId,
        comment_id: CommentId,
        reply: &str,
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.store.author();
        let mut issue = self.store.get_raw(issue_id)?;
        let changes = events::reply(&mut issue, comment_id, &author, reply, Timestamp::now())?;

        self.store.update(*issue_id, "Reply", changes, signer)?;

        Ok(())
    }

    /// Get all issues, sorted by time.
    pub fn all(&self) -> Result<Vec<(IssueId, Issue)>, Error> {
        let mut issues = self.store.list()?;
        issues.sort_by_key(|(_, i)| i.timestamp);

        Ok(issues)
    }

    /// Get the issue count.
    pub fn count(&self) -> Result<usize, Error> {
        let issues = self.store.list()?;

        Ok(issues.len())
    }
}

/// Issue events.
mod events {
    use super::*;
    use automerge::{
        transaction::{CommitOptions, Transactable},
        ObjId,
    };

    pub fn create(
        author: &Author,
        title: &str,
        description: &str,
        timestamp: Timestamp,
        labels: &[Label],
    ) -> Result<Contents, TransactionError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(TransactionError::InvalidValue("title"));
        }

        let mut doc = Automerge::new();
        let _issue = doc
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Create issue".to_owned()),
                |tx| {
                    let issue = tx.put_object(ObjId::Root, "issue", ObjType::Map)?;

                    tx.put(&issue, "title", title)?;
                    tx.put(&issue, "author", author)?;
                    tx.put(&issue, "state", State::Open)?;
                    tx.put(&issue, "timestamp", timestamp)?;
                    tx.put_object(&issue, "discussion", ObjType::List)?;

                    let labels_id = tx.put_object(&issue, "labels", ObjType::Map)?;
                    for label in labels {
                        tx.put(&labels_id, label.name().trim(), true)?;
                    }

                    // Nb. The top-level comment doesn't have a `replies` field.
                    let comment_id = tx.put_object(&issue, "comment", ObjType::Map)?;

                    tx.put(&comment_id, "body", description.trim())?;
                    tx.put(&comment_id, "author", author)?;
                    tx.put(&comment_id, "timestamp", timestamp)?;
                    tx.put_object(&comment_id, "reactions", ObjType::Map)?;

                    Ok(issue)
                },
            )
            .map_err(|failure| failure.error)?
            .result;

        Ok(doc.save_incremental())
    }

    pub fn comment(
        issue: &mut Automerge,
        author: &Author,
        body: &str,
        timestamp: Timestamp,
    ) -> Result<Contents, TransactionError> {
        let _comment = issue
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Add comment".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_obj, obj_id) = tx.get(ObjId::Root, "issue")?;
                    let (_, discussion_id) = tx.get(&obj_id, "discussion")?;

                    let length = tx.length(&discussion_id);
                    let comment = tx.insert_object(&discussion_id, length, ObjType::Map)?;

                    tx.put(&comment, "author", author)?;
                    tx.put(&comment, "body", body.trim())?;
                    tx.put(&comment, "timestamp", timestamp)?;
                    tx.put_object(&comment, "replies", ObjType::List)?;
                    tx.put_object(&comment, "reactions", ObjType::Map)?;

                    Ok(comment)
                },
            )
            .map_err(|failure| failure.error)?
            .result;

        #[allow(clippy::unwrap_used)]
        let change = issue.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(change)
    }

    pub fn lifecycle(
        issue: &mut Automerge,
        author: &Author,
        state: State,
    ) -> Result<Contents, TransactionError> {
        issue
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message(state.lifecycle_message()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "issue")?;
                    tx.put(&obj_id, "state", state)?;
                    tx.put(&obj_id, "author", author)?;

                    Ok(())
                },
            )
            .map_err(|failure| failure.error)?;

        #[allow(clippy::unwrap_used)]
        let change = issue.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(change)
    }

    pub fn label(
        issue: &mut Automerge,
        _author: &Author,
        labels: &[Label],
    ) -> Result<Contents, TransactionError> {
        issue
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Label issue".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "issue")?;
                    let (_, labels_id) = tx.get(&obj_id, "labels")?;

                    for label in labels {
                        tx.put(&labels_id, label.name().trim(), true)?;
                    }
                    Ok(())
                },
            )
            .map_err(|failure| failure.error)?;

        #[allow(clippy::unwrap_used)]
        let change = issue.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(change)
    }

    pub fn reply(
        issue: &mut Automerge,
        comment_id: CommentId,
        author: &Author,
        body: &str,
        timestamp: Timestamp,
    ) -> Result<Contents, TransactionError> {
        issue
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Reply".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "issue")?;
                    let (_, discussion_id) = tx.get(&obj_id, "discussion")?;
                    let (_, comment_id) = tx.get(&discussion_id, usize::from(comment_id))?;
                    let (_, replies_id) = tx.get(&comment_id, "replies")?;

                    let length = tx.length(&replies_id);
                    let reply = tx.insert_object(&replies_id, length, ObjType::Map)?;

                    // Nb. Replies don't themselves have replies.
                    tx.put(&reply, "author", author)?;
                    tx.put(&reply, "body", body.trim())?;
                    tx.put(&reply, "timestamp", timestamp)?;
                    tx.put_object(&reply, "reactions", ObjType::Map)?;

                    Ok(())
                },
            )
            .map_err(|failure| failure.error)?;

        #[allow(clippy::unwrap_used)]
        let change = issue.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(change)
    }

    pub fn react(
        issue: &mut Automerge,
        comment_id: CommentId,
        author: &Author,
        reactions: &[Reaction],
    ) -> Result<Contents, TransactionError> {
        issue
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("React".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "issue")?;
                    let (_, discussion_id) = tx.get(&obj_id, "discussion")?;
                    let (_, comment_id) = if comment_id == CommentId::root() {
                        tx.get(&obj_id, "comment")?
                    } else {
                        tx.get(&discussion_id, usize::from(comment_id) - 1)?
                    };
                    let (_, reactions_id) = tx.get(&comment_id, "reactions")?;

                    for reaction in reactions {
                        let key = reaction.emoji.to_string();
                        let reaction_id = if let Some((_, reaction_id)) =
                            tx.try_get(&reactions_id, key)?
                        {
                            reaction_id
                        } else {
                            tx.put_object(&reactions_id, reaction.emoji.to_string(), ObjType::Map)?
                        };
                        tx.put(&reaction_id, author.id.to_human(), true)?;
                    }

                    Ok(())
                },
            )
            .map_err(|failure| failure.error)?;

        #[allow(clippy::unwrap_used)]
        let change = issue.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(change)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test;

    #[test]
    fn test_issue_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let timestamp = Timestamp::now();
        let issue_id = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();
        let issue = issues.get(&issue_id).unwrap().unwrap();

        assert_eq!(issue.title(), "My first issue");
        assert_eq!(issue.author(), &store.author());
        assert_eq!(issue.description(), "Blah blah blah.");
        assert_eq!(issue.comments().len(), 0);
        assert_eq!(issue.state(), State::Open);
        assert!(issue.timestamp() >= timestamp);
    }

    #[test]
    fn test_issue_create_and_change_state() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let issue_id = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        issues
            .lifecycle(
                &issue_id,
                State::Closed {
                    reason: CloseReason::Other,
                },
                &signer,
            )
            .unwrap();

        let issue = issues.get(&issue_id).unwrap().unwrap();
        assert_eq!(
            issue.state(),
            State::Closed {
                reason: CloseReason::Other
            }
        );

        issues.lifecycle(&issue_id, State::Open, &signer).unwrap();
        let issue = issues.get(&issue_id).unwrap().unwrap();
        assert_eq!(issue.state(), State::Open);
    }

    #[test]
    fn test_issue_react() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let issue_id = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        let reaction = Reaction::new('🥳').unwrap();
        issues
            .react(&issue_id, CommentId::root(), reaction, &signer)
            .unwrap();

        let issue = issues.get(&issue_id).unwrap().unwrap();
        let count = issue.reactions()[&reaction];

        // TODO: Test multiple reactions from same author and different authors

        assert_eq!(count, 1);
    }

    #[test]
    fn test_issue_reply() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let issue_id = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        issues.comment(&issue_id, "Ho ho ho.", &signer).unwrap();
        issues
            .reply(&issue_id, CommentId::root(), "Hi hi hi.", &signer)
            .unwrap();
        issues
            .reply(&issue_id, CommentId::root(), "Ha ha ha.", &signer)
            .unwrap();

        let issue = issues.get(&issue_id).unwrap().unwrap();
        let reply1 = &issue.comments()[0].replies[0];
        let reply2 = &issue.comments()[0].replies[1];

        assert_eq!(reply1.body, "Hi hi hi.");
        assert_eq!(reply2.body, "Ha ha ha.");
    }

    #[test]
    fn test_issue_label() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let issue_id = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        let bug_label = Label::new("bug").unwrap();
        let wontfix_label = Label::new("wontfix").unwrap();

        issues
            .label(&issue_id, &[bug_label.clone()], &signer)
            .unwrap();
        issues
            .label(&issue_id, &[wontfix_label.clone()], &signer)
            .unwrap();

        let issue = issues.get(&issue_id).unwrap().unwrap();
        let labels = issue.labels();

        assert!(labels.contains(&bug_label));
        assert!(labels.contains(&wontfix_label));
    }

    #[test]
    fn test_issue_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let now = Timestamp::now();
        let author = *signer.public_key();
        let issue_id = issues
            .create("My first issue", "Blah blah blah.", &[], &signer)
            .unwrap();

        issues.comment(&issue_id, "Ho ho ho.", &signer).unwrap();
        issues.comment(&issue_id, "Ha ha ha.", &signer).unwrap();

        let issue = issues.get(&issue_id).unwrap().unwrap();
        let c1 = &issue.comments()[0];
        let c2 = &issue.comments()[1];

        assert_eq!(&c1.body, "Ho ho ho.");
        assert_eq!(c1.author.id(), &author);
        assert_eq!(&c2.body, "Ha ha ha.");
        assert_eq!(c2.author.id(), &author);
        assert!(c1.timestamp >= now);
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
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let issues = store.issues();
        let author = store.author();

        let contents =
            events::create(&author, "First", "Blah blah.", Timestamp::new(0), &[]).unwrap();
        issues
            .store
            .create("Create issue", contents, &signer)
            .unwrap();

        let contents =
            events::create(&author, "Second", "Blah blah.", Timestamp::new(1), &[]).unwrap();
        issues
            .store
            .create("Create issue", contents, &signer)
            .unwrap();

        let contents =
            events::create(&author, "Third", "Blah blah.", Timestamp::new(2), &[]).unwrap();
        issues
            .store
            .create("Create issue", contents, &signer)
            .unwrap();

        let issues = issues.all().unwrap();
        assert_eq!(issues.len(), 3);

        // Issues are sorted by timestamp.
        assert_eq!(issues[0].1.title(), "First");
        assert_eq!(issues[1].1.title(), "Second");
        assert_eq!(issues[2].1.title(), "Third");
    }
}
