use std::str::FromStr;

use crypto::{PublicKey, Signature};
use once_cell::sync::Lazy;
use radicle_cob::TypeName;
use radicle_crdt::{clock, GMap, LWWMap, LWWReg, Max, Redactable, Semilattice};
use radicle_crypto::Verified;
use radicle_git_ext::Oid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    cob::{self, common::Timestamp, store},
    identity::{doc::DocError, Identity},
    prelude::Doc,
    storage::RemoteId,
};

use super::{
    thread::{self, Thread},
    Author, OpId,
};

/// The logical clock we use to order operations to patches.
pub use clock::Lamport as Clock;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.identity.proposal").expect("type name is valid"));

pub type Op = cob::Op<Action>;

pub type RevisionId = OpId;

/// Proposal operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Accept {
        revision: RevisionId,
        signature: Signature,
    },
    Edit {
        title: String,
        description: String,
    },
    Redact {
        revision: RevisionId,
    },
    Reject {
        revision: RevisionId,
    },
    Revision {
        proposed: Doc<Verified>,
        previous: Identity<Oid>,
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
    #[error("the revision {0:?} is redacted")]
    Redacted(OpId),
    /// Error applying an op to the patch thread.
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::OpError),
}

/// Error publishing the proposal.
#[derive(Error, Debug)]
pub enum PublishError {
    #[error("the revision {0:?} is missing")]
    Missing(OpId),
    #[error("the revision {0:?} is redacted")]
    Redacted(OpId),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("signatures did not reach quorum threshold: {0}")]
    Quorum(usize),
}

/// Propose a new [`Doc`] for an [`Identity`]. The proposal can be
/// reviewed by gathering [`Signature`]s for accepting the changes, or
/// rejecting them.
///
/// Once a proposal has reached the quourum threshold for the previous
/// [`Identity`] then it may be published to the person's local
/// storage using [`Proposal::publish`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Title of the proposal.
    title: LWWReg<Max<String>>,
    /// Description of the proposal.
    description: LWWReg<Max<String>>,
    /// List of revisions for this proposal.
    revisions: GMap<RevisionId, Redactable<Revision>>,
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
            revisions: GMap::default(),
        }
    }
}

impl Proposal {
    /// Publish the [`Doc`], found at the given `revision`, to the
    /// provided `remote`.
    ///
    /// # Errors
    ///
    /// This operation will fail if:
    ///   * The `revision` is missing
    ///   * The `revision` is redacted
    ///   * The number of signatures for this revision does not reach
    ///     the quorum for the previous [`Doc`].
    pub fn publish(
        &self,
        revision: &RevisionId,
        remote: &RemoteId,
        repo: &git2::Repository,
    ) -> Result<Identity<Oid>, PublishError> {
        let revision = self
            .revision(revision)
            .ok_or_else(|| PublishError::Missing(*revision))?
            .get()
            .ok_or_else(|| PublishError::Redacted(*revision))?;
        let doc = &revision.proposed;

        if !revision.reaches_quorum() {
            return Err(PublishError::Quorum(doc.threshold));
        }

        let signatures = revision.signatures();
        let msg = format!(
            "{}\n\n{}",
            self.title(),
            self.description().unwrap_or_default()
        );
        let current = doc.update(remote, &msg, &signatures.collect::<Vec<_>>(), repo)?;

        Ok(Identity {
            head: current,
            root: revision.previous.root,
            current,
            revision: revision.previous.revision + 1,
            doc: doc.clone(),
            signatures: revision
                .signatures()
                .map(|(key, sig)| (*key, sig))
                .collect(),
        })
    }

    /// The most recent title for the proposal.
    pub fn title(&self) -> &str {
        self.title.get().get()
    }

    /// The most recent description for the proposal, if present.
    pub fn description(&self) -> Option<&str> {
        Some(self.description.get().get())
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
                Action::Edit { title, description } => {
                    self.title.set(title, op.clock);
                    self.description.set(description, op.clock);
                }
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
                Action::Revision { proposed, previous } => self.revisions.insert(
                    id,
                    Redactable::Present(Revision::new(author, previous, proposed, timestamp)),
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
    /// Previous [`Identity`] that is going to be updated.
    pub previous: Identity<Oid>,
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
        previous: Identity<Oid>,
        proposed: Doc<Verified>,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            author,
            previous,
            proposed,
            discussion: Thread::default(),
            verdicts: LWWMap::default(),
            timestamp,
        }
    }

    pub fn signatures(&self) -> impl Iterator<Item = (&PublicKey, Signature)> {
        self.verdicts().filter_map(|(key, verdict)| match verdict {
            Verdict::Accept(sig) => Some((key, sig.clone())),
            Verdict::Reject => None,
        })
    }

    pub fn verdicts(&self) -> impl Iterator<Item = (&PublicKey, &Verdict)> {
        self.verdicts
            .iter()
            .filter_map(|(key, verdict)| verdict.get().map(|verdict| (key, verdict)))
    }

    pub fn reaches_quorum(&self) -> bool {
        let votes_for = self
            .verdicts
            .iter()
            .fold(0, |count, (_, verdict)| match verdict.get() {
                Some(Verdict::Accept(_)) => count + 1,
                Some(Verdict::Reject) => count,
                None => count,
            });
        votes_for >= self.previous.doc.threshold
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
