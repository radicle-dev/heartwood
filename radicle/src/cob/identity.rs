use std::collections::BTreeMap;
use std::{ops::Deref, str::FromStr};

use crypto::{PublicKey, Signature};
use once_cell::sync::Lazy;
use radicle_cob::{ObjectId, TypeName};
use radicle_crypto::{Signer, Verified};
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    cob::{
        self,
        store::{self, FromHistory as _, HistoryAction, Transaction},
        Timestamp,
    },
    identity::{doc::DocError, Did, Identity, IdentityError},
    prelude::{Doc, ReadRepository},
    storage::{RemoteId, WriteRepository},
};

use super::{
    thread::{self, Thread},
    Author, EntryId,
};

/// Type name of an identity proposal.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.id.proposal").expect("type name is valid"));

pub type Op = cob::Op<Action>;

pub type ProposalId = ObjectId;

pub type RevisionId = EntryId;

/// Proposal operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Accept {
        revision: RevisionId,
        signature: Signature,
    },
    Close,
    Edit {
        title: String,
        description: String,
    },
    Commit,
    Redact {
        revision: RevisionId,
    },
    Reject {
        revision: RevisionId,
    },
    Revision {
        // N.b. the `Oid` is a blob identifier and not a commit, so we
        // do not need to propagate it via HistoryAction.
        current: Oid,
        proposed: Doc<Verified>,
    },
    Thread {
        revision: RevisionId,
        action: thread::Action,
    },
}

impl HistoryAction for Action {}

/// Error applying an operation onto a state.
#[derive(Error, Debug)]
pub enum ApplyError {
    /// Causal dependency missing.
    ///
    /// This error indicates that the operations are not being applied
    /// in causal order, which is a requirement for this CRDT.
    ///
    /// For example, this can occur if an operation references anothern operation
    /// that hasn't happened yet.
    #[error("causal dependency {0:?} missing")]
    Missing(EntryId),
    #[error("the proposal is committed")]
    Committed,
    #[error(transparent)]
    Commit(#[from] CommitError),
    #[error("the revision {0:?} is redacted")]
    Redacted(EntryId),
    /// Error applying an op to the proposal thread.
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::Error),
    /// Error validating the state.
    #[error("validation failed: {0}")]
    Validate(&'static str),
}

/// Error committing the proposal.
#[derive(Error, Debug)]
pub enum CommitError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("the proposal {0} is closed")]
    Closed(EntryId),
    #[error("the revision {0} is missing")]
    Missing(EntryId),
    #[error(
        "the identity hashes do match '{current} =/= {expected}' for the revision '{revision}'"
    )]
    Mismatch {
        current: Oid,
        expected: Oid,
        revision: EntryId,
    },
    #[error("the revision {0} is already committed")]
    Committed(EntryId),
    #[error("the revision {0} is redacted")]
    Redacted(EntryId),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("signatures did not reach quorum threshold: {0}")]
    Quorum(usize),
}

/// Error updating or creating proposals.
#[derive(Error, Debug)]
pub enum Error {
    #[error("apply failed: {0}")]
    Apply(#[from] ApplyError),
    #[error("store: {0}")]
    Store(#[from] store::Error),
}

/// Propose a new [`Doc`] for an [`Identity`]. The proposal can be
/// reviewed by gathering [`Signature`]s for accepting the changes, or
/// rejecting them.
///
/// Once a proposal has reached the quourum threshold for the previous
/// [`Identity`] then it may be committed to the person's local
/// storage using [`Proposal::commit`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Proposal {
    /// Title of the proposal.
    title: String,
    /// Description of the proposal.
    description: String,
    /// Current state of the proposal.
    state: State,
    /// List of revisions for this proposal.
    revisions: BTreeMap<RevisionId, Option<Revision>>,
    /// Timeline of events.
    timeline: Vec<EntryId>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    #[default]
    Open,
    Closed,
    Committed,
}

impl Proposal {
    /// Commit the [`Doc`], found at the given `revision`, to the
    /// provided `remote`.
    ///
    /// # Errors
    ///
    /// This operation will fail if:
    ///   * The `revision` is missing
    ///   * The `revision` is redacted
    ///   * The number of signatures for this revision does not reach
    ///     the quorum for the previous [`Doc`].
    pub fn commit<R, G>(
        &self,
        rid: &RevisionId,
        remote: &RemoteId,
        repo: &R,
        signer: &G,
    ) -> Result<Identity<Oid>, CommitError>
    where
        R: WriteRepository,
        G: Signer,
    {
        match self.state() {
            State::Closed => return Err(CommitError::Closed(*rid)),
            State::Committed => return Err(CommitError::Committed(*rid)),
            State::Open => {}
        }
        let revision = self
            .revision(rid)
            .ok_or_else(|| CommitError::Missing(*rid))?
            .as_ref()
            .ok_or_else(|| CommitError::Redacted(*rid))?;
        let doc = &revision.proposed;
        let previous = Identity::load(signer.public_key(), repo)?;

        if previous.current != revision.current {
            return Err(CommitError::Mismatch {
                current: revision.current,
                expected: previous.current,
                revision: *rid,
            });
        }

        if !revision.is_quorum_reached(&previous) {
            return Err(CommitError::Quorum(doc.threshold));
        }

        let signatures = revision.signatures();
        let msg = format!(
            "{}\n\n{}",
            self.title(),
            self.description().unwrap_or_default()
        );
        let current = doc.update(remote, &msg, &signatures.collect::<Vec<_>>(), repo.raw())?;
        let head = repo.set_identity_head()?;

        assert_eq!(head, current);

        Ok(Identity {
            head,
            root: previous.root,
            current,
            revision: previous.revision + 1,
            doc: doc.clone(),
            signatures: revision
                .signatures()
                .map(|(key, sig)| (*key, sig))
                .collect(),
        })
    }

    pub fn is_committed(&self) -> bool {
        match self.state() {
            State::Open => false,
            State::Closed => false,
            State::Committed => true,
        }
    }

    /// The most recent title for the proposal.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The most recent description for the proposal, if present.
    pub fn description(&self) -> Option<&str> {
        Some(self.description.as_str())
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// A specific [`Revision`], that may be redacted.
    pub fn revision(&self, revision: &RevisionId) -> Option<&Option<Revision>> {
        self.revisions.get(revision)
    }

    /// All the [`Revision`]s that have not been redacted.
    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = (&RevisionId, &Revision)> {
        self.timeline.iter().filter_map(|id| {
            self.revisions
                .get(id)
                .and_then(|o| o.as_ref())
                .map(|rev| (id, rev))
        })
    }

    pub fn latest_by(&self, who: &Did) -> Option<(&RevisionId, &Revision)> {
        self.revisions().rev().find_map(|(rid, r)| {
            if r.author.id() == who {
                Some((rid, r))
            } else {
                None
            }
        })
    }

    pub fn latest(&self) -> Option<(&RevisionId, &Revision)> {
        self.revisions().next_back()
    }
}

impl store::FromHistory for Proposal {
    type Action = Action;
    type Error = ApplyError;

    fn type_name() -> &'static TypeName {
        &*TYPENAME
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.revisions.is_empty() {
            return Err(ApplyError::Validate("no revisions found"));
        }
        Ok(())
    }

    fn apply<R: ReadRepository>(&mut self, op: Op, repo: &R) -> Result<(), Self::Error> {
        let id = op.id;
        let author = Author::new(op.author);
        let timestamp = op.timestamp;

        debug_assert!(!self.timeline.contains(&op.id));

        self.timeline.push(id);

        for action in op.actions {
            match action {
                Action::Accept {
                    revision,
                    signature,
                } => match self.revisions.get_mut(&revision) {
                    Some(Some(revision)) => revision.accept(op.author, signature),
                    Some(None) => return Err(ApplyError::Redacted(revision)),
                    None => return Err(ApplyError::Missing(revision)),
                },
                Action::Close => self.state = State::Closed,
                Action::Edit { title, description } => {
                    self.title = title;
                    self.description = description;
                }
                Action::Commit => self.state = State::Committed,
                Action::Redact { revision } => {
                    if let Some(revision) = self.revisions.get_mut(&revision) {
                        *revision = None;
                    } else {
                        return Err(ApplyError::Missing(revision));
                    }
                }
                Action::Reject { revision } => match self.revisions.get_mut(&revision) {
                    Some(Some(revision)) => revision.reject(op.author),
                    Some(None) => return Err(ApplyError::Redacted(revision)),
                    None => return Err(ApplyError::Missing(revision)),
                },
                Action::Revision { current, proposed } => {
                    // Since revisions are keyed by content hash, we shouldn't re-insert a revision
                    // if it already exists, otherwise this will be resolved via the `merge`
                    // operation of `Redactable`.
                    if self.revisions.contains_key(&id) {
                        continue;
                    }
                    self.revisions.insert(
                        id,
                        Some(Revision::new(author.clone(), current, proposed, timestamp)),
                    );
                }

                Action::Thread { revision, action } => match self.revisions.get_mut(&revision) {
                    Some(Some(revision)) => revision.discussion.apply(
                        cob::Op::new(op.id, action, op.author, op.timestamp, op.identity),
                        repo,
                    )?,
                    Some(None) => return Err(ApplyError::Redacted(revision)),
                    None => return Err(ApplyError::Missing(revision)),
                },
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// An accepting verdict must supply the [`Signature`] over the
    /// new proposed [`Doc`].
    Accept(Signature),
    /// Rejecting the proposed [`Doc`].
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    /// Author of this proposed revision.
    pub author: Author,
    /// [`Identity::current`]'s current [`Oid`] that this revision was
    /// based on.
    pub current: Oid,
    /// New [`Doc`] that will replace `previous`' document.
    pub proposed: Doc<Verified>,
    /// Discussion thread for this revision.
    pub discussion: Thread,
    /// [`Verdict`]s given by the delegates.
    pub verdicts: BTreeMap<PublicKey, Option<Verdict>>,
    /// Physical timestamp of this proposal revision.
    pub timestamp: Timestamp,
}

impl Revision {
    pub fn new(
        author: Author,
        current: Oid,
        proposed: Doc<Verified>,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            author,
            current,
            proposed,
            discussion: Thread::default(),
            verdicts: BTreeMap::default(),
            timestamp,
        }
    }

    pub fn signatures(&self) -> impl Iterator<Item = (&PublicKey, Signature)> {
        self.verdicts().filter_map(|(key, verdict)| match verdict {
            Verdict::Accept(sig) => Some((key, *sig)),
            Verdict::Reject => None,
        })
    }

    pub fn verdicts(&self) -> impl Iterator<Item = (&PublicKey, &Verdict)> {
        self.verdicts
            .iter()
            .filter_map(|(key, verdict)| verdict.as_ref().map(|verdict| (key, verdict)))
    }

    pub fn accepted(&self) -> Vec<Did> {
        self.verdicts()
            .filter_map(|(key, v)| match v {
                Verdict::Accept(_) => Some(key.into()),
                Verdict::Reject => None,
            })
            .collect()
    }

    pub fn rejected(&self) -> Vec<Did> {
        self.verdicts()
            .filter_map(|(key, v)| match v {
                Verdict::Accept(_) => None,
                Verdict::Reject => Some(key.into()),
            })
            .collect()
    }

    pub fn is_quorum_reached(&self, previous: &Identity<Oid>) -> bool {
        let votes_for = self
            .verdicts
            .iter()
            .fold(0, |count, (_, verdict)| match verdict {
                Some(Verdict::Accept(_)) => count + 1,
                Some(Verdict::Reject) => count,
                None => count,
            });
        votes_for >= previous.doc.threshold
    }

    fn accept(&mut self, key: PublicKey, signature: Signature) {
        self.verdicts.insert(key, Some(Verdict::Accept(signature)));
    }

    fn reject(&mut self, key: PublicKey) {
        self.verdicts.insert(key, Some(Verdict::Reject));
    }
}

impl store::Transaction<Proposal> {
    pub fn accept(
        &mut self,
        revision: RevisionId,
        signature: Signature,
    ) -> Result<(), store::Error> {
        self.push(Action::Accept {
            revision,
            signature,
        })
    }

    pub fn reject(&mut self, revision: RevisionId) -> Result<(), store::Error> {
        self.push(Action::Reject { revision })
    }

    pub fn edit(
        &mut self,
        title: impl ToString,
        description: impl ToString,
    ) -> Result<(), store::Error> {
        self.push(Action::Edit {
            title: title.to_string(),
            description: description.to_string(),
        })
    }

    pub fn redact(&mut self, revision: RevisionId) -> Result<(), store::Error> {
        self.push(Action::Redact { revision })
    }

    pub fn revision(&mut self, current: Oid, proposed: Doc<Verified>) -> Result<(), store::Error> {
        self.push(Action::Revision { current, proposed })
    }

    /// Start a proposal revision discussion.
    pub fn thread<S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
    ) -> Result<(), store::Error> {
        self.push(Action::Thread {
            revision,
            action: thread::Action::Comment {
                body: body.to_string(),
                reply_to: None,
            },
        })
    }

    /// Comment on a proposal revision.
    pub fn comment<S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        reply_to: thread::CommentId,
    ) -> Result<(), store::Error> {
        self.push(Action::Thread {
            revision,
            action: thread::Action::Comment {
                body: body.to_string(),
                reply_to: Some(reply_to),
            },
        })
    }
}

pub struct ProposalMut<'a, 'g, R> {
    pub id: ObjectId,

    proposal: Proposal,
    store: &'g mut Proposals<'a, R>,
}

impl<'a, 'g, R> ProposalMut<'a, 'g, R>
where
    R: WriteRepository + cob::Store,
{
    pub fn new(id: ObjectId, proposal: Proposal, store: &'g mut Proposals<'a, R>) -> Self {
        Self {
            id,
            proposal,
            store,
        }
    }

    pub fn transaction<G, F>(
        &mut self,
        message: &str,
        signer: &G,
        operations: F,
    ) -> Result<EntryId, Error>
    where
        G: Signer,
        F: FnOnce(&mut Transaction<Proposal>) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::new(*signer.public_key());
        operations(&mut tx)?;
        let (ops, commit) = tx.commit(message, self.id, &mut self.store.raw, signer)?;

        self.proposal.apply(ops, self.store.as_ref())?;

        Ok(commit)
    }

    /// Accept a proposal revision.
    pub fn accept<G: Signer>(
        &mut self,
        revision: RevisionId,
        signature: Signature,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Accept", signer, |tx| tx.accept(revision, signature))
    }

    /// Reject a proposal revision.
    pub fn reject<G: Signer>(
        &mut self,
        revision: RevisionId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Reject", signer, |tx| tx.reject(revision))
    }

    /// Edit proposal metadata.
    pub fn edit<G: Signer>(
        &mut self,
        title: String,
        description: String,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit", signer, |tx| tx.edit(title, description))
    }

    /// Commit a proposal.
    pub fn commit<G: Signer>(&mut self, signer: &G) -> Result<EntryId, Error> {
        self.transaction("Commit", signer, |tx| tx.push(Action::Commit))
    }

    /// Close a proposal.
    pub fn close<G: Signer>(&mut self, signer: &G) -> Result<EntryId, Error> {
        self.transaction("Close", signer, |tx| tx.push(Action::Close))
    }

    /// Comment on a proposal revision.
    pub fn comment<G: Signer, S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        reply_to: thread::CommentId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Comment", signer, |tx| tx.comment(revision, body, reply_to))
    }

    /// Update a proposal with new metadata.
    pub fn update<G: Signer>(
        &mut self,
        current: impl Into<Oid>,
        proposed: Doc<Verified>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Add revision", signer, |tx| {
            tx.revision(current.into(), proposed)
        })
    }
}

impl<'a, 'g, R> Deref for ProposalMut<'a, 'g, R> {
    type Target = Proposal;

    fn deref(&self) -> &Self::Target {
        &self.proposal
    }
}

pub struct Proposals<'a, R> {
    raw: store::Store<'a, Proposal, R>,
}

impl<'a, R> Deref for Proposals<'a, R> {
    type Target = store::Store<'a, Proposal, R>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a, R: WriteRepository> Proposals<'a, R>
where
    R: WriteRepository + cob::Store,
{
    /// Open a proposals store.
    pub fn open(repository: &'a R) -> Result<Self, store::Error> {
        let raw = store::Store::open(repository)?;

        Ok(Self { raw })
    }

    /// Create a proposal.
    pub fn create<'g, G: Signer>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        current: impl Into<Oid>,
        proposed: Doc<Verified>,
        signer: &G,
    ) -> Result<ProposalMut<'a, 'g, R>, Error> {
        let (id, proposal) =
            Transaction::initial("Create proposal", &mut self.raw, signer, |tx| {
                tx.revision(current.into(), proposed)?;
                tx.edit(title, description)?;

                Ok(())
            })?;

        Ok(ProposalMut::new(id, proposal, self))
    }

    /// Get a proposal.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Proposal>, store::Error> {
        self.raw.get(id)
    }

    /// Get a proposal mutably.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<ProposalMut<'a, 'g, R>, store::Error> {
        let proposal = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(ProposalMut {
            id: *id,
            proposal,
            store: self,
        })
    }
}

#[cfg(test)]
mod test {
    use super::State;

    #[test]
    fn test_ordering() {
        assert!(State::Committed > State::Closed);
        assert!(State::Committed > State::Open);
        assert!(State::Closed > State::Open);
    }
}
