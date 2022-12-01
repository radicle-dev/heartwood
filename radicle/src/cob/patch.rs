#![allow(clippy::too_many_arguments)]
use std::fmt;
use std::ops::ControlFlow;
use std::ops::Deref;
use std::ops::Range;
use std::str::FromStr;

use once_cell::sync::Lazy;
use radicle_crdt::clock;
use radicle_crdt::{GMap, LWWReg, LWWSet, Max, Redactable, Semilattice};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Author, Tag, Timestamp};
use crate::cob::thread;
use crate::cob::thread::CommentId;
use crate::cob::thread::Thread;
use crate::cob::{store, ActorId, ObjectId, OpId, TypeName};
use crate::crypto::{PublicKey, Signer};
use crate::git;
use crate::prelude::*;
use crate::storage::git as storage;

/// The logical clock we use to order operations to patches.
pub use clock::Lamport as Clock;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.patch").expect("type name is valid"));

/// Patch operation.
pub type Op = crate::cob::Op<Action>;

/// Identifier for a patch.
pub type PatchId = ObjectId;

/// Unique identifier for a patch revision.
pub type RevisionId = OpId;

/// Index of a revision in the revisions list.
pub type RevisionIx = usize;

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
}

/// Error updating or creating patches.
#[derive(Error, Debug)]
pub enum Error {
    #[error("apply failed: {0}")]
    Apply(#[from] ApplyError),
    #[error("store: {0}")]
    Store(#[from] store::Error),
}

/// Patch operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Action {
    Edit {
        title: String,
        description: String,
        target: MergeTarget,
    },
    Tag {
        add: Vec<Tag>,
        remove: Vec<Tag>,
    },
    Revision {
        base: git::Oid,
        oid: git::Oid,
    },
    Redact {
        revision: RevisionId,
    },
    Review {
        revision: RevisionId,
        comment: Option<String>,
        verdict: Option<Verdict>,
        inline: Vec<CodeComment>,
    },
    Merge {
        revision: RevisionId,
        commit: git::Oid,
    },
    Thread {
        revision: RevisionId,
        action: thread::Action,
    },
}

/// Where a patch is intended to be merged.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeTarget {
    /// Intended for the default branch of the project delegates.
    /// Note that if the delegations change while the patch is open,
    /// this will always mean whatever the "current" delegation set is.
    /// If it were otherwise, patches could become un-mergeable.
    #[default]
    Delegates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// Title of the patch.
    pub title: LWWReg<Max<String>>,
    /// Patch description.
    pub description: LWWReg<Max<String>>,
    /// Current status of the patch.
    pub status: LWWReg<Max<Status>>,
    /// Target this patch is meant to be merged in.
    pub target: LWWReg<Max<MergeTarget>>,
    /// Associated tags.
    pub tags: LWWSet<Tag>,
    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    pub revisions: GMap<RevisionId, Redactable<Revision>>,
}

impl Semilattice for Patch {
    fn merge(&mut self, other: Self) {
        self.title.merge(other.title);
        self.description.merge(other.description);
        self.status.merge(other.status);
        self.target.merge(other.target);
        self.tags.merge(other.tags);
        self.revisions.merge(other.revisions);
    }
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            title: Max::from(String::default()).into(),
            description: Max::from(String::default()).into(),
            status: Max::from(Status::default()).into(),
            target: Max::from(MergeTarget::default()).into(),
            tags: LWWSet::default(),
            revisions: GMap::default(),
        }
    }
}

impl Patch {
    pub fn title(&self) -> &str {
        self.title.get().get()
    }

    pub fn status(&self) -> Status {
        *self.status.get().get()
    }

    pub fn target(&self) -> MergeTarget {
        *self.target.get().get()
    }

    pub fn timestamp(&self) -> Timestamp {
        self.revisions()
            .next()
            .map(|(_, r)| r)
            .expect("Patch::timestamp: at least one revision is present")
            .timestamp
    }

    pub fn description(&self) -> Option<&str> {
        Some(self.description.get().get())
    }

    pub fn author(&self) -> &Author {
        &self
            .revisions()
            .next()
            .map(|(_, r)| r)
            .expect("Patch::author: at least one revision is present")
            .author
    }

    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = (&RevisionId, &Revision)> {
        self.revisions
            .iter()
            .filter_map(|(rid, r)| -> Option<(&RevisionId, &Revision)> {
                r.get().map(|r| (rid, r))
            })
    }

    pub fn head(&self) -> &git::Oid {
        &self
            .latest()
            .map(|(_, r)| r)
            .expect("Patch::head: at least one revision is present")
            .oid
    }

    pub fn version(&self) -> RevisionIx {
        self.revisions
            .len()
            .checked_sub(1)
            .expect("Patch::version: at least one revision is present")
    }

    pub fn latest(&self) -> Option<(&RevisionId, &Revision)> {
        self.revisions().next_back()
    }

    pub fn is_proposed(&self) -> bool {
        matches!(self.status.get().get(), Status::Proposed)
    }

    pub fn is_archived(&self) -> bool {
        matches!(self.status.get().get(), &Status::Archived)
    }

    /// Apply a list of operations to the state.
    pub fn apply(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<(), ApplyError> {
        for op in ops {
            self.apply_one(op)?;
        }
        Ok(())
    }

    /// Apply a single op to the state.
    pub fn apply_one(&mut self, op: Op) -> Result<(), ApplyError> {
        let id = op.id();
        let author = Author::new(op.author);
        let timestamp = op.timestamp;

        match op.action {
            Action::Edit {
                title,
                description,
                target,
            } => {
                self.title.set(title, op.clock);
                self.description.set(description, op.clock);
                self.target.set(target, op.clock);
            }
            Action::Tag { add, remove } => {
                for tag in add {
                    self.tags.insert(tag, op.clock);
                }
                for tag in remove {
                    self.tags.remove(tag, op.clock);
                }
            }
            Action::Revision { base, oid } => {
                self.revisions.insert(
                    id,
                    Redactable::Present(Revision::new(author, base, oid, timestamp)),
                );
            }
            Action::Redact { revision } => {
                if let Some(revision) = self.revisions.get_mut(&revision) {
                    revision.merge(Redactable::Redacted);
                } else {
                    return Err(ApplyError::Missing(revision));
                }
            }
            Action::Review {
                revision,
                ref comment,
                verdict,
                ref inline,
            } => {
                if let Some(Redactable::Present(revision)) = self.revisions.get_mut(&revision) {
                    revision.reviews.insert(
                        op.author,
                        Review::new(verdict, comment.to_owned(), inline.to_owned(), timestamp),
                    );
                } else {
                    return Err(ApplyError::Missing(revision));
                }
            }
            Action::Merge { revision, commit } => {
                if let Some(Redactable::Present(revision)) = self.revisions.get_mut(&revision) {
                    revision.merges.insert(
                        Merge {
                            node: op.author,
                            commit,
                            timestamp,
                        }
                        .into(),
                        op.clock,
                    );
                } else {
                    return Err(ApplyError::Missing(revision));
                }
            }
            Action::Thread { revision, action } => {
                // TODO(cloudhead): Make sure we can deal with redacted revisions which are added
                // to out of order, like in the `Merge` case.
                if let Some(Redactable::Present(revision)) = self.revisions.get_mut(&revision) {
                    revision.discussion.apply([cob::Op {
                        action,
                        author: op.author,
                        clock: op.clock,
                        timestamp,
                    }]);
                } else {
                    return Err(ApplyError::Missing(revision));
                }
            }
        }
        Ok(())
    }
}

impl store::FromHistory for Patch {
    type Action = Action;

    fn type_name() -> &'static TypeName {
        &*TYPENAME
    }

    fn from_history(
        history: &radicle_cob::History,
    ) -> Result<(Self, clock::Lamport), store::Error> {
        let obj = history.traverse(Self::default(), |mut acc, entry| {
            if let Ok(op) = Op::try_from(entry) {
                if let Err(err) = acc.apply([op]) {
                    log::warn!("Error applying op to patch state: {err}");
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

/// A patch revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// Author of the revision.
    pub author: Author,
    /// Base branch commit, used as a merge base.
    pub base: git::Oid,
    /// Reference to the Git object containing the code (revision head).
    pub oid: git::Oid,
    /// Discussion around this revision.
    pub discussion: Thread,
    /// Merges of this revision into other repositories.
    pub merges: LWWSet<Max<Merge>>,
    /// Reviews of this revision's changes (one per actor).
    pub reviews: GMap<ActorId, Review>,
    /// When this revision was created.
    pub timestamp: Timestamp,
}

impl Revision {
    pub fn new(author: Author, base: git::Oid, oid: git::Oid, timestamp: Timestamp) -> Self {
        Self {
            author,
            base,
            oid,
            discussion: Thread::default(),
            merges: LWWSet::default(),
            reviews: GMap::default(),
            timestamp,
        }
    }

    pub fn description(&self) -> Option<&str> {
        self.discussion.first()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Proposed,
    Draft,
    Archived,
}

/// A merged patch revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Merge {
    /// Owner of repository that this patch was merged into.
    pub node: NodeId,
    /// Base branch commit that contains the revision.
    pub commit: git::Oid,
    /// When this merged was performed.
    pub timestamp: Timestamp,
}

/// A patch review verdict.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Accept patch.
    Accept,
    /// Reject patch.
    Reject,
}

impl Semilattice for Verdict {
    fn merge(&mut self, other: Self) {
        if self == &Self::Accept && other == Self::Reject {
            *self = other;
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept => write!(f, "accept"),
            Self::Reject => write!(f, "reject"),
        }
    }
}

/// Code location, used for attaching comments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLocation {
    /// File being commented on.
    pub blob: git::Oid,
    /// Commit commented on.
    pub commit: git::Oid,
    /// Line range commented on.
    pub lines: Range<usize>,
}

impl PartialOrd for CodeLocation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CodeLocation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.blob, &self.commit, &self.lines.start, &self.lines.end).cmp(&(
            &other.blob,
            &other.commit,
            &other.lines.start,
            &other.lines.end,
        ))
    }
}

/// Comment on code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CodeComment {
    /// Code location of the comment.
    pub location: CodeLocation,
    /// Comment.
    pub comment: String,
    /// Timestamp.
    pub timestamp: Timestamp,
}

/// A patch review on a revision.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Review {
    /// Review verdict.
    pub verdict: LWWReg<Option<Verdict>>,
    /// Review general comment.
    pub comment: LWWReg<Option<Max<String>>>,
    /// Review inline code comments.
    pub inline: LWWSet<Max<CodeComment>>,
    /// Review timestamp.
    pub timestamp: Max<Timestamp>,
}

impl Semilattice for Review {
    fn merge(&mut self, other: Self) {
        self.verdict.merge(other.verdict);
        self.comment.merge(other.comment);
        self.inline.merge(other.inline);
        self.timestamp.merge(other.timestamp);
    }
}

impl Review {
    pub fn new(
        verdict: Option<Verdict>,
        comment: Option<String>,
        inline: Vec<CodeComment>,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            verdict: LWWReg::from(verdict),
            comment: LWWReg::from(comment.map(Max::from)),
            inline: LWWSet::from_iter(
                inline
                    .into_iter()
                    .map(Max::from)
                    .zip(std::iter::repeat(clock::Lamport::default())),
            ),
            timestamp: Max::from(timestamp),
        }
    }

    pub fn verdict(&self) -> Option<Verdict> {
        self.verdict.get().as_ref().copied()
    }

    pub fn comment(&self) -> Option<&str> {
        self.comment.get().as_ref().map(|m| m.get().as_str())
    }

    pub fn timestamp(&self) -> Timestamp {
        *self.timestamp.get()
    }
}

pub struct PatchMut<'a, 'g> {
    pub id: ObjectId,

    patch: Patch,
    clock: clock::Lamport,
    store: &'g mut Patches<'a>,
}

impl<'a, 'g> PatchMut<'a, 'g> {
    pub fn new(
        id: ObjectId,
        patch: Patch,
        clock: clock::Lamport,
        store: &'g mut Patches<'a>,
    ) -> Self {
        Self {
            id,
            clock,
            patch,
            store,
        }
    }

    /// Get the internal logical clock.
    pub fn clock(&self) -> &clock::Lamport {
        &self.clock
    }

    /// Edit patch metadata.
    pub fn edit<G: Signer>(
        &mut self,
        title: String,
        description: String,
        target: MergeTarget,
        signer: &G,
    ) -> Result<OpId, Error> {
        let action = Action::Edit {
            title,
            description,
            target,
        };
        self.apply("Edit", action, signer)
    }

    /// Comment on a patch revision.
    pub fn comment<G: Signer, S: Into<String>>(
        &mut self,
        revision: RevisionId,
        body: S,
        signer: &G,
    ) -> Result<CommentId, Error> {
        let body = body.into();
        let action = Action::Thread {
            revision,
            action: thread::Action::Comment {
                body,
                reply_to: None,
            },
        };
        self.apply("Comment", action, signer)
    }

    /// Review a patch revision.
    pub fn review<G: Signer>(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        comment: Option<String>,
        inline: Vec<CodeComment>,
        signer: &G,
    ) -> Result<OpId, Error> {
        let action = Action::Review {
            revision,
            comment,
            verdict,
            inline,
        };
        self.apply("Review patch", action, signer)
    }

    /// Merge a patch revision.
    pub fn merge<G: Signer>(
        &mut self,
        revision: RevisionId,
        commit: git::Oid,
        signer: &G,
    ) -> Result<OpId, Error> {
        let action = Action::Merge { revision, commit };
        self.apply("Merge revision", action, signer)
    }

    /// Update a patch with a new revision.
    pub fn update<G: Signer>(
        &mut self,
        description: impl Into<String>,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        signer: &G,
    ) -> Result<OpId, Error> {
        let description = description.into();
        let base = base.into();
        let oid = oid.into();
        let revision = self.apply(
            "Update patch with new revision",
            Action::Revision { base, oid },
            signer,
        )?;
        self.comment(revision, description, signer)?;

        Ok(revision)
    }

    /// Tag a patch.
    pub fn tag<G: Signer>(
        &mut self,
        add: impl IntoIterator<Item = Tag>,
        remove: impl IntoIterator<Item = Tag>,
        signer: &G,
    ) -> Result<OpId, Error> {
        let add = add.into_iter().collect::<Vec<_>>();
        let remove = remove.into_iter().collect::<Vec<_>>();
        let action = Action::Tag { add, remove };

        self.apply("Tag", action, signer)
    }

    /// Apply an operation to the patch.
    pub fn apply<G: Signer>(
        &mut self,
        msg: &'static str,
        action: Action,
        signer: &G,
    ) -> Result<OpId, Error> {
        let cob = self
            .store
            .update(self.id, msg, action.clone(), signer)
            .map_err(Error::Store)?;
        let clock = cob.history().clock().into();
        let timestamp = cob.history().timestamp().into();
        let op = Op {
            action,
            author: *signer.public_key(),
            clock,
            timestamp,
        };
        self.patch.apply_one(op)?;

        Ok((clock, *signer.public_key()))
    }
}

impl<'a, 'g> Deref for PatchMut<'a, 'g> {
    type Target = Patch;

    fn deref(&self) -> &Self::Target {
        &self.patch
    }
}

pub struct Patches<'a> {
    raw: store::Store<'a, Patch>,
}

impl<'a> Deref for Patches<'a> {
    type Target = store::Store<'a, Patch>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a> Patches<'a> {
    /// Open an patches store.
    pub fn open(
        whoami: PublicKey,
        repository: &'a storage::Repository,
    ) -> Result<Self, store::Error> {
        let raw = store::Store::open(whoami, repository)?;

        Ok(Self { raw })
    }

    /// Create a patch.
    pub fn create<'g, G: Signer>(
        &'g mut self,
        title: impl Into<String>,
        description: impl Into<String>,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        tags: &[Tag],
        signer: &G,
    ) -> Result<PatchMut<'a, 'g>, Error> {
        let title = title.into();
        let description = description.into();
        let action = Action::Revision {
            base: base.into(),
            oid: oid.into(),
        };
        let (id, patch, clock) = self.raw.create("Create patch", action, signer)?;
        let mut patch = PatchMut::new(id, patch, clock, self);

        patch.edit(title, description, target, signer)?;
        patch.tag(tags.to_owned(), [], signer)?;

        Ok(patch)
    }

    /// Get an issue.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Patch>, store::Error> {
        self.raw.get(id).map(|r| r.map(|(p, _)| p))
    }

    /// Get an issue mutably.
    pub fn get_mut<'g>(&'g mut self, id: &ObjectId) -> Result<PatchMut<'a, 'g>, store::Error> {
        let (patch, clock) = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(PatchMut {
            id: *id,
            clock,
            patch,
            store: self,
        })
    }

    /// Get proposed patches.
    pub fn proposed(
        &self,
    ) -> Result<impl Iterator<Item = (PatchId, Patch, clock::Lamport)>, Error> {
        let all = self.all()?;

        Ok(all
            .into_iter()
            .filter_map(|result| result.ok())
            .filter(|(_, p, _)| p.is_proposed()))
    }

    /// Get patches proposed by the given key.
    pub fn proposed_by<'b>(
        &'b self,
        who: &'b PublicKey,
    ) -> Result<impl Iterator<Item = (PatchId, Patch, clock::Lamport)> + '_, Error> {
        Ok(self
            .proposed()?
            .filter(move |(_, p, _)| p.author().id() == who))
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;
    use std::{array, iter};

    use radicle_crdt::test::{assert_laws, WeightedGenerator};

    use pretty_assertions::assert_eq;
    use quickcheck::{Arbitrary, TestResult};

    use super::*;
    use crate::cob::op::{Actor, ActorId};
    use crate::crypto::test::signer::MockSigner;
    use crate::test;

    #[derive(Clone)]
    struct Changes<const N: usize> {
        permutations: [Vec<Op>; N],
    }

    impl<const N: usize> std::fmt::Debug for Changes<N> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for (i, p) in self.permutations.iter().enumerate() {
                writeln!(
                    f,
                    "{i}: {:#?}",
                    p.iter().map(|c| &c.action).collect::<Vec<_>>()
                )?;
            }
            Ok(())
        }
    }

    impl<const N: usize> Arbitrary for Changes<N> {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            type State = (clock::Lamport, Vec<OpId>, Vec<Tag>);

            let author = ActorId::from([0; 32]);
            let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
            let oids = iter::repeat_with(|| {
                git::Oid::try_from(
                    iter::repeat_with(|| rng.u8(..))
                        .take(20)
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .unwrap()
            })
            .take(16)
            .collect::<Vec<_>>();

            let gen = WeightedGenerator::<(clock::Lamport, Action), State>::new(rng.clone())
                .variant(1, |(clock, _, _), rng| {
                    Some((
                        clock.tick(),
                        Action::Edit {
                            title: iter::repeat_with(|| rng.alphabetic()).take(8).collect(),
                            description: iter::repeat_with(|| rng.alphabetic()).take(16).collect(),
                            target: MergeTarget::Delegates,
                        },
                    ))
                })
                .variant(1, |(clock, revisions, _), rng| {
                    if revisions.is_empty() {
                        return None;
                    }
                    let revision = revisions[rng.usize(..revisions.len())];
                    let commit = oids[rng.usize(..oids.len())];

                    Some((clock.tick(), Action::Merge { revision, commit }))
                })
                .variant(1, |(clock, revisions, _), rng| {
                    if revisions.is_empty() {
                        return None;
                    }
                    let revision = revisions[rng.usize(..revisions.len())];

                    Some((clock.tick(), Action::Redact { revision }))
                })
                .variant(1, |(clock, _, tags), rng| {
                    let add = iter::repeat_with(|| rng.alphabetic())
                        .take(rng.usize(0..=3))
                        .map(|c| Tag::new(c).unwrap())
                        .collect::<Vec<_>>();
                    let remove = tags
                        .iter()
                        .take(rng.usize(0..=tags.len()))
                        .cloned()
                        .collect();
                    for tag in &add {
                        tags.push(tag.clone());
                    }
                    Some((clock.tick(), Action::Tag { add, remove }))
                })
                .variant(1, |(clock, revisions, _), rng| {
                    let oid = oids[rng.usize(..oids.len())];
                    let base = oids[rng.usize(..oids.len())];

                    if rng.bool() {
                        revisions.push((clock.tick(), author));
                    }
                    Some((*clock, Action::Revision { base, oid }))
                });

            let mut changes = Vec::new();
            let mut permutations: [Vec<Op>; N] = array::from_fn(|_| Vec::new());
            let timestamp = Timestamp::now() + rng.u64(..60);

            for (clock, action) in gen.take(g.size()) {
                changes.push(Op {
                    action,
                    author,
                    clock,
                    timestamp,
                });
            }

            for p in &mut permutations {
                *p = changes.clone();
                rng.shuffle(&mut changes);
            }

            Changes { permutations }
        }
    }

    #[test]
    fn prop_invariants() {
        fn property(log: Changes<3>) -> TestResult {
            let t = Patch::default();
            let [p1, p2, p3] = log.permutations;

            let mut t1 = t.clone();
            match t1.apply(p1) {
                Ok(()) => {}
                Err(ApplyError::Missing(_)) => return TestResult::discard(),
            }

            let mut t2 = t.clone();
            match t2.apply(p2) {
                Ok(()) => {}
                Err(ApplyError::Missing(_)) => return TestResult::discard(),
            }

            let mut t3 = t;
            match t3.apply(p3) {
                Ok(()) => {}
                Err(ApplyError::Missing(_)) => return TestResult::discard(),
            }

            assert_eq!(t1, t2);
            assert_eq!(t2, t3);
            assert_laws(&t1, &t2, &t3);

            TestResult::passed()
        }

        quickcheck::QuickCheck::new()
            .min_tests_passed(100)
            .gen(quickcheck::Gen::new(8))
            .quickcheck(property as fn(Changes<3>) -> TestResult);
    }

    #[test]
    fn test_patch_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut patches = Patches::open(*signer.public_key(), &project).unwrap();
        let author = *signer.public_key();
        let target = MergeTarget::Delegates;
        let oid = git::Oid::from_str("e2a85016a458cd809c0ecee81f8c99613b0b0945").unwrap();
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                target,
                base,
                oid,
                &[],
                &signer,
            )
            .unwrap();

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();

        assert_eq!(patch.title(), "My first patch");
        assert_eq!(patch.description(), Some("Blah blah blah."));
        assert_eq!(patch.author().id(), &author);
        assert_eq!(patch.status(), Status::Proposed);
        assert_eq!(patch.target(), target);
        assert_eq!(patch.version(), 0);

        let (_, revision) = patch.latest().unwrap();

        assert_eq!(revision.author.id(), &author);
        assert_eq!(revision.description(), None);
        assert_eq!(revision.discussion.len(), 0);
        assert_eq!(revision.oid, oid);
        assert_eq!(revision.base, base);
    }

    #[test]
    fn test_patch_merge() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let oid = git::Oid::from_str("e2a85016a458cd809c0ecee81f8c99613b0b0945").unwrap();
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let mut patches = Patches::open(*signer.public_key(), &project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                base,
                oid,
                &[],
                &signer,
            )
            .unwrap();

        let id = patch.id;
        let (rid, _) = patch.revisions().next().unwrap();
        let _merge = patch.merge(*rid, base, &signer).unwrap();

        let patch = patches.get(&id).unwrap().unwrap();

        let (_, r) = patch.revisions().next().unwrap();
        let merges = r.merges.iter().collect::<Vec<_>>();
        assert_eq!(merges.len(), 1);

        let merge = merges.first().unwrap();
        assert_eq!(merge.node, *signer.public_key());
        assert_eq!(merge.commit, base);
    }

    #[test]
    fn test_patch_review() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut patches = Patches::open(*signer.public_key(), &project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                base,
                oid,
                &[],
                &signer,
            )
            .unwrap();

        let (rid, _) = patch.latest().unwrap();
        patch
            .review(
                *rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                &signer,
            )
            .unwrap();

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();
        let (_, revision) = patch.latest().unwrap();
        assert_eq!(revision.reviews.len(), 1);

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.comment(), Some("LGTM"));
    }

    #[test]
    fn test_revision_redacted() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut alice = Actor::<_, Action>::new(MockSigner::default());
        let mut patch = Patch::default();

        let a1 = alice.op(Action::Revision { base, oid });
        let a2 = alice.op(Action::Redact { revision: a1.id() });
        let a3 = alice.op(Action::Review {
            revision: a1.id(),
            comment: None,
            verdict: Some(Verdict::Accept),
            inline: vec![],
        });
        let a4 = alice.op(Action::Merge {
            revision: a1.id(),
            commit: oid,
        });

        patch.apply([a1]).unwrap();
        assert!(patch.revisions().next().is_some());

        patch.apply([a2]).unwrap();
        assert!(patch.revisions().next().is_none());

        patch.apply([a3]).unwrap_err();
        patch.apply([a4]).unwrap_err();
    }

    #[test]
    fn test_revision_redacted_reinsert() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut alice = Actor::<_, Action>::new(MockSigner::default());
        let mut p1 = Patch::default();
        let mut p2 = Patch::default();

        let a1 = alice.op(Action::Revision { base, oid });
        let a2 = alice.op(Action::Redact { revision: a1.id() });

        p1.apply([a1.clone(), a2.clone(), a1.clone()]).unwrap();
        p2.apply([a1.clone(), a1, a2]).unwrap();

        assert_eq!(p1, p2);
    }

    #[test]
    fn test_patch_review_edit() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut patches = Patches::open(*signer.public_key(), &project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                base,
                oid,
                &[],
                &signer,
            )
            .unwrap();

        let (rid, _) = patch.latest().unwrap();
        let rid = *rid;

        patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                &signer,
            )
            .unwrap();
        patch
            .review(rid, Some(Verdict::Reject), None, vec![], &signer)
            .unwrap(); // Overwrite the verdict.

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest().unwrap();
        assert_eq!(revision.reviews.len(), 1, "the reviews were merged");

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.comment(), Some("LGTM"));

        patch
            .review(rid, None, Some("Whoops!".to_owned()), vec![], &signer)
            .unwrap(); // Overwrite the comment.
        let (_, revision) = patch.latest().unwrap();
        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.comment(), Some("Whoops!"));
    }

    #[test]
    fn test_patch_update() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("af08e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let rev0_oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let rev1_oid = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let mut patches = Patches::open(*signer.public_key(), &project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                base,
                rev0_oid,
                &[],
                &signer,
            )
            .unwrap();

        assert_eq!(patch.description(), Some("Blah blah blah."));
        assert_eq!(patch.version(), 0);

        let _rev1_id = patch
            .update("I've made changes.", base, rev1_oid, &signer)
            .unwrap();

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();
        assert_eq!(patch.version(), 1);
        assert_eq!(patch.revisions.len(), 2);

        let (_, revision) = patch.latest().unwrap();

        assert_eq!(patch.version(), 1);
        assert_eq!(revision.oid, rev1_oid);
        assert_eq!(revision.description(), Some("I've made changes."));
    }
}
