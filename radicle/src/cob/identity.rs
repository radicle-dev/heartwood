use std::{ops::Deref, str::FromStr};

use crypto::{PublicKey, Signature};
use once_cell::sync::Lazy;
use radicle_cob::{ObjectId, TypeName};
use radicle_crdt::{clock, GMap, LWWMap, LWWReg, Max, Redactable, Semilattice};
use radicle_crypto::{Signer, Verified};
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    cob::{
        self,
        common::Timestamp,
        store::{self, FromHistory as _, Transaction},
    },
    identity::{doc::DocError, Identity, IdentityError},
    prelude::Doc,
    storage::{git as storage, RemoteId, WriteRepository},
};

use super::{
    thread::{self, Thread},
    Author, OpId,
};

/// The logical clock we use to order operations to proposals.
pub use clock::Lamport as Clock;

/// Type name of an identity proposal.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.id.proposal").expect("type name is valid"));

pub type Op = cob::Op<Action>;

pub type ProposalId = ObjectId;

pub type RevisionId = OpId;

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
        current: Oid,
        proposed: Doc<Verified>,
    },
    Thread {
        revision: RevisionId,
        action: thread::Action,
    },
}

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
    Missing(OpId),
    #[error("the proposal is committed")]
    Committed,
    #[error(transparent)]
    Commit(#[from] CommitError),
    #[error("the revision {0:?} is redacted")]
    Redacted(OpId),
    /// Error applying an op to the proposal thread.
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::OpError),
}

/// Error committing the proposal.
#[derive(Error, Debug)]
pub enum CommitError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("the proposal {0} is closed")]
    Closed(OpId),
    #[error("the revision {0} is missing")]
    Missing(OpId),
    #[error(
        "the identity hashes do match '{current} =/= {expected}' for the revision '{revision}'"
    )]
    Mismatch {
        current: Oid,
        expected: Oid,
        revision: OpId,
    },
    #[error("the revision {0} is already committed")]
    Committed(OpId),
    #[error("the revision {0} is redacted")]
    Redacted(OpId),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Title of the proposal.
    title: LWWReg<Max<String>>,
    /// Description of the proposal.
    description: LWWReg<Max<String>>,
    /// Current state of the proposal.
    state: LWWReg<Max<State>>,
    /// List of revisions for this proposal.
    revisions: GMap<RevisionId, Redactable<Revision>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    #[default]
    Open,
    Closed,
    Committed,
}

impl Semilattice for Proposal {
    fn merge(&mut self, other: Self) {
        self.description.merge(other.description);
        self.revisions.merge(other.revisions);
    }
}

impl Default for Proposal {
    fn default() -> Self {
        Self {
            title: Max::from(String::default()).into(),
            description: Max::from(String::default()).into(),
            state: Max::from(State::default()).into(),
            revisions: GMap::default(),
        }
    }
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
            .get()
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

        Ok(Identity {
            head: current,
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
        self.title.get().get()
    }

    /// The most recent description for the proposal, if present.
    pub fn description(&self) -> Option<&str> {
        Some(self.description.get().get())
    }

    pub fn state(&self) -> &State {
        self.state.get().get()
    }

    /// A specific [`Revision`], that may be redacted.
    pub fn revision(&self, revision: &RevisionId) -> Option<&Redactable<Revision>> {
        self.revisions.get(revision)
    }

    /// All the [`Revision`]s that have not been redacted.
    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = (&RevisionId, &Revision)> {
        self.revisions
            .iter()
            .filter_map(|(rid, r)| -> Option<(&RevisionId, &Revision)> {
                r.get().map(|r| (rid, r))
            })
    }

    pub fn latest_by(&self, who: &PublicKey) -> Option<(&RevisionId, &Revision)> {
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

    fn apply(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<(), Self::Error> {
        for op in ops {
            let id = op.id();
            let author = Author::new(op.author);
            let timestamp = op.timestamp;

            match op.action {
                Action::Accept {
                    revision,
                    signature,
                } => match self.revisions.get_mut(&revision) {
                    Some(Redactable::Present(revision)) => {
                        revision.accept(op.author, signature, op.clock)
                    }
                    Some(Redactable::Redacted) => return Err(ApplyError::Redacted(revision)),
                    None => return Err(ApplyError::Missing(revision)),
                },
                Action::Close => self.state.set(State::Closed, op.clock),
                Action::Edit { title, description } => {
                    self.title.set(title, op.clock);
                    self.description.set(description, op.clock);
                }
                Action::Commit => self.state.set(State::Committed, op.clock),
                Action::Redact { revision } => {
                    if let Some(revision) = self.revisions.get_mut(&revision) {
                        revision.merge(Redactable::Redacted);
                    } else {
                        return Err(ApplyError::Missing(revision));
                    }
                }
                Action::Reject { revision } => match self.revisions.get_mut(&revision) {
                    Some(Redactable::Present(revision)) => revision.reject(op.author, op.clock),
                    Some(Redactable::Redacted) => return Err(ApplyError::Redacted(revision)),
                    None => return Err(ApplyError::Missing(revision)),
                },
                Action::Revision { current, proposed } => self.revisions.insert(
                    id,
                    Redactable::Present(Revision::new(author, current, proposed, timestamp)),
                ),
                Action::Thread { revision, action } => match self.revisions.get_mut(&revision) {
                    Some(Redactable::Present(revision)) => revision
                        .discussion
                        .apply([cob::Op::new(action, op.author, op.timestamp, op.clock)])?,
                    Some(Redactable::Redacted) => return Err(ApplyError::Redacted(revision)),
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
    pub verdicts: LWWMap<PublicKey, Redactable<Verdict>>,
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
            verdicts: LWWMap::default(),
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
            .filter_map(|(key, verdict)| verdict.get().map(|verdict| (key, verdict)))
    }

    pub fn accepted(&self) -> Vec<PublicKey> {
        self.verdicts()
            .filter_map(|(key, v)| match v {
                Verdict::Accept(_) => Some(*key),
                Verdict::Reject => None,
            })
            .collect()
    }

    pub fn rejected(&self) -> Vec<PublicKey> {
        self.verdicts()
            .filter_map(|(key, v)| match v {
                Verdict::Accept(_) => None,
                Verdict::Reject => Some(*key),
            })
            .collect()
    }

    pub fn is_quorum_reached(&self, previous: &Identity<Oid>) -> bool {
        let votes_for = self
            .verdicts
            .iter()
            .fold(0, |count, (_, verdict)| match verdict.get() {
                Some(Verdict::Accept(_)) => count + 1,
                Some(Verdict::Reject) => count,
                None => count,
            });
        votes_for >= previous.doc.threshold
    }

    fn accept(&mut self, key: PublicKey, signature: Signature, clock: Clock) {
        self.verdicts
            .insert(key, Redactable::Present(Verdict::Accept(signature)), clock);
    }

    fn reject(&mut self, key: PublicKey, clock: Clock) {
        self.verdicts
            .insert(key, Redactable::Present(Verdict::Reject), clock)
    }
}

impl store::Transaction<Proposal> {
    pub fn accept(&mut self, revision: RevisionId, signature: Signature) -> OpId {
        self.push(Action::Accept {
            revision,
            signature,
        })
    }

    pub fn reject(&mut self, revision: RevisionId) -> OpId {
        self.push(Action::Reject { revision })
    }

    pub fn edit(&mut self, title: impl ToString, description: impl ToString) -> OpId {
        self.push(Action::Edit {
            title: title.to_string(),
            description: description.to_string(),
        })
    }

    pub fn redact(&mut self, revision: RevisionId) -> OpId {
        self.push(Action::Redact { revision })
    }

    pub fn revision(&mut self, current: Oid, proposed: Doc<Verified>) -> OpId {
        self.push(Action::Revision { current, proposed })
    }

    /// Start a proposal revision discussion.
    pub fn thread<S: ToString>(&mut self, revision: RevisionId, body: S) -> OpId {
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
    ) -> OpId {
        self.push(Action::Thread {
            revision,
            action: thread::Action::Comment {
                body: body.to_string(),
                reply_to: Some(reply_to),
            },
        })
    }
}

pub struct ProposalMut<'a, 'g> {
    pub id: ObjectId,

    proposal: Proposal,
    clock: clock::Lamport,
    store: &'g mut Proposals<'a>,
}

impl<'a, 'g> ProposalMut<'a, 'g> {
    pub fn new(
        id: ObjectId,
        proposal: Proposal,
        clock: clock::Lamport,
        store: &'g mut Proposals<'a>,
    ) -> Self {
        Self {
            id,
            clock,
            proposal,
            store,
        }
    }

    pub fn transaction<G, F, T>(
        &mut self,
        message: &str,
        signer: &G,
        operations: F,
    ) -> Result<T, Error>
    where
        G: Signer,
        F: FnOnce(&mut Transaction<Proposal>) -> T,
    {
        let mut tx = Transaction::new(*signer.public_key(), self.clock);
        let output = operations(&mut tx);
        let (ops, clock) = tx.commit(message, self.id, &mut self.store.raw, signer)?;

        self.proposal.apply(ops)?;
        self.clock = clock;

        Ok(output)
    }

    /// Get the internal logical clock.
    pub fn clock(&self) -> &clock::Lamport {
        &self.clock
    }

    /// Accept a proposal revision.
    pub fn accept<G: Signer>(
        &mut self,
        revision: RevisionId,
        signature: Signature,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("Accept", signer, |tx| tx.accept(revision, signature))
    }

    /// Reject a proposal revision.
    pub fn reject<G: Signer>(&mut self, revision: RevisionId, signer: &G) -> Result<OpId, Error> {
        self.transaction("Reject", signer, |tx| tx.reject(revision))
    }

    /// Edit proposal metadata.
    pub fn edit<G: Signer>(
        &mut self,
        title: String,
        description: String,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("Edit", signer, |tx| tx.edit(title, description))
    }

    /// Commit a proposal.
    pub fn commit<G: Signer>(&mut self, signer: &G) -> Result<OpId, Error> {
        self.transaction("Commit", signer, |tx| tx.push(Action::Commit))
    }

    /// Close a proposal.
    pub fn close<G: Signer>(&mut self, signer: &G) -> Result<OpId, Error> {
        self.transaction("Close", signer, |tx| tx.push(Action::Close))
    }

    /// Comment on a proposal revision.
    pub fn comment<G: Signer, S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        reply_to: thread::CommentId,
        signer: &G,
    ) -> Result<thread::CommentId, Error> {
        self.transaction("Comment", signer, |tx| tx.comment(revision, body, reply_to))
    }

    /// Update a proposal with new metadata.
    pub fn update<G: Signer>(
        &mut self,
        current: impl Into<Oid>,
        proposed: Doc<Verified>,
        signer: &G,
    ) -> Result<OpId, Error> {
        self.transaction("Add revision", signer, |tx| {
            tx.revision(current.into(), proposed)
        })
    }
}

impl<'a, 'g> Deref for ProposalMut<'a, 'g> {
    type Target = Proposal;

    fn deref(&self) -> &Self::Target {
        &self.proposal
    }
}

pub struct Proposals<'a> {
    raw: store::Store<'a, Proposal>,
}

impl<'a> Deref for Proposals<'a> {
    type Target = store::Store<'a, Proposal>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a> Proposals<'a> {
    /// Open a proposals store.
    pub fn open(
        whoami: PublicKey,
        repository: &'a storage::Repository,
    ) -> Result<Self, store::Error> {
        let raw = store::Store::open(whoami, repository)?;

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
    ) -> Result<ProposalMut<'a, 'g>, Error> {
        let (id, proposal, clock) =
            Transaction::initial("Create proposal", &mut self.raw, signer, |tx| {
                tx.revision(current.into(), proposed);
                tx.edit(title, description);
            })?;
        // Just a sanity check that our clock is advancing as expected.
        debug_assert_eq!(clock.get(), 2);

        Ok(ProposalMut::new(id, proposal, clock, self))
    }

    /// Get a proposal.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Proposal>, store::Error> {
        self.raw.get(id).map(|r| r.map(|(p, _)| p))
    }

    /// Get a proposal mutably.
    pub fn get_mut<'g>(&'g mut self, id: &ObjectId) -> Result<ProposalMut<'a, 'g>, store::Error> {
        let (proposal, clock) = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(ProposalMut {
            id: *id,
            clock,
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
