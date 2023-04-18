#![allow(clippy::too_many_arguments)]
use std::fmt;
use std::ops::Deref;
use std::ops::Range;
use std::path::PathBuf;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use radicle_crdt::clock;
use radicle_crdt::{GMap, GSet, LWWReg, LWWSet, Lamport, Max, Redactable, Semilattice};

use crate::cob;
use crate::cob::common::{Author, Tag, Timestamp};
use crate::cob::store::Transaction;
use crate::cob::store::{FromHistory as _, HistoryAction};
use crate::cob::thread;
use crate::cob::thread::CommentId;
use crate::cob::thread::Thread;
use crate::cob::{store, ActorId, EntryId, ObjectId, TypeName};
use crate::crypto::{PublicKey, Signer};
use crate::git;
use crate::identity::doc::DocError;
use crate::prelude::*;
use crate::storage::git as storage;

/// The logical clock we use to order operations to patches.
pub use clock::Lamport as Clock;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.patch").expect("type name is valid"));

/// Patch operation.
pub type Op = cob::Op<Action>;

/// Identifier for a patch.
pub type PatchId = ObjectId;

/// Unique identifier for a patch revision.
pub type RevisionId = EntryId;

/// Index of a revision in the revisions list.
pub type RevisionIx = usize;

/// Error applying an operation onto a state.
#[derive(Debug, Error)]
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
    /// Error applying an op to the patch thread.
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::OpError),
    /// Error loading the identity document committed to by an operation.
    #[error("identity doc failed to load: {0}")]
    Doc(#[from] DocError),
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
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Edit {
        title: String,
        description: String,
        target: MergeTarget,
    },
    EditRevision {
        revision: RevisionId,
        description: String,
    },
    Tag {
        add: Vec<Tag>,
        remove: Vec<Tag>,
    },
    Revision {
        description: String,
        base: git::Oid,
        oid: git::Oid,
    },
    Lifecycle {
        state: State,
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

impl HistoryAction for Action {
    fn parents(&self) -> Vec<git::Oid> {
        match self {
            Self::Revision { base, oid, .. } => {
                vec![*base, *oid]
            }
            Self::Merge { commit, .. } => {
                vec![*commit]
            }
            _ => vec![],
        }
    }
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
    title: LWWReg<Max<String>>,
    /// Patch description.
    description: LWWReg<Max<String>>,
    /// Current state of the patch.
    state: LWWReg<Max<State>>,
    /// Target this patch is meant to be merged in.
    target: LWWReg<Max<MergeTarget>>,
    /// Associated tags.
    tags: LWWSet<Tag>,
    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    revisions: GMap<RevisionId, Redactable<Revision>>,
    /// Timeline of operations.
    timeline: GSet<(Lamport, EntryId)>,
}

impl Semilattice for Patch {
    fn merge(&mut self, other: Self) {
        self.title.merge(other.title);
        self.description.merge(other.description);
        self.state.merge(other.state);
        self.target.merge(other.target);
        self.tags.merge(other.tags);
        self.revisions.merge(other.revisions);
    }
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            title: LWWReg::initial(Max::from(String::default())),
            description: LWWReg::initial(Max::from(String::default())),
            state: LWWReg::initial(Max::from(State::default())),
            target: LWWReg::initial(Max::from(MergeTarget::default())),
            tags: LWWSet::default(),
            revisions: GMap::default(),
            timeline: GSet::default(),
        }
    }
}

impl Patch {
    /// Title of the patch.
    pub fn title(&self) -> &str {
        self.title.get().get()
    }

    /// Current state of the patch.
    pub fn state(&self) -> State {
        *self.state.get().get()
    }

    /// Target this patch is meant to be merged in.
    pub fn target(&self) -> MergeTarget {
        *self.target.get().get()
    }

    /// Timestamp of the first revision of the patch.
    pub fn timestamp(&self) -> Timestamp {
        self.revisions()
            .next()
            .map(|(_, r)| r)
            .expect("Patch::timestamp: at least one revision is present")
            .timestamp
    }

    /// Associated tags.
    pub fn tags(&self) -> impl Iterator<Item = &Tag> {
        self.tags.iter()
    }

    /// Patch description.
    pub fn description(&self) -> &str {
        self.description.get().get()
    }

    /// Author of the first revision of the patch.
    pub fn author(&self) -> &Author {
        &self
            .revisions()
            .next()
            .map(|(_, r)| r)
            .expect("Patch::author: at least one revision is present")
            .author
    }

    /// Get the `Revision` by its `RevisionId`.
    ///
    /// None is returned if the `Revision` has been redacted (deleted).
    pub fn revision(&self, id: &RevisionId) -> Option<&Revision> {
        self.revisions.get(id).and_then(Redactable::get)
    }

    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = (&RevisionId, &Revision)> {
        self.timeline.iter().filter_map(|(_, id)| {
            self.revisions
                .get(id)
                .and_then(Redactable::get)
                .map(|rev| (id, rev))
        })
    }

    /// Reference to the Git object containing the code on the latest revision.
    pub fn head(&self) -> &git::Oid {
        &self
            .latest()
            .map(|(_, r)| r)
            .expect("Patch::head: at least one revision is present")
            .oid
    }

    /// Index of latest revision in the revisions list.
    pub fn version(&self) -> RevisionIx {
        self.revisions
            .len()
            .checked_sub(1)
            .expect("Patch::version: at least one revision is present")
    }

    /// Latest revision.
    pub fn latest(&self) -> Option<(&RevisionId, &Revision)> {
        self.revisions().next_back()
    }

    /// Check if the patch is open.
    pub fn is_open(&self) -> bool {
        matches!(self.state.get().get(), State::Open)
    }

    /// Check if the patch is archived.
    pub fn is_archived(&self) -> bool {
        matches!(self.state.get().get(), &State::Archived)
    }
}

impl store::FromHistory for Patch {
    type Action = Action;
    type Error = ApplyError;

    fn type_name() -> &'static TypeName {
        &*TYPENAME
    }

    fn apply<R: ReadRepository>(
        &mut self,
        ops: impl IntoIterator<Item = Op>,
        repo: &R,
    ) -> Result<(), ApplyError> {
        for op in ops {
            let id = op.id;
            let author = Author::new(op.author);
            let timestamp = op.timestamp;

            self.timeline.insert((op.clock, id));

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
                Action::EditRevision {
                    revision,
                    description,
                } => {
                    if let Some(Redactable::Present(revision)) = self.revisions.get_mut(&revision) {
                        revision.description.set(description, op.clock);
                    } else {
                        return Err(ApplyError::Missing(revision));
                    }
                }
                Action::Revision {
                    description,
                    base,
                    oid,
                } => {
                    // Since revisions are keyed by content hash, we shouldn't re-insert a revision
                    // if it already exists, otherwise this will be resolved via the `merge`
                    // operation of `Redactable`.
                    if self.revisions.contains_key(&id) {
                        continue;
                    }
                    self.revisions.insert(
                        id,
                        Redactable::Present(Revision::new(
                            author,
                            description,
                            base,
                            oid,
                            timestamp,
                            op.clock,
                        )),
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
                            Review::new(
                                verdict,
                                comment.to_owned(),
                                inline.to_owned(),
                                timestamp,
                                op.clock,
                            ),
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
                        let doc = repo.identity_doc_at(op.identity)?;

                        if revision
                            .merges()
                            .filter(|m| doc.is_delegate(&m.node))
                            .count()
                            >= doc.threshold
                        {
                            self.state.set(State::Merged, op.clock);
                        }
                    } else {
                        return Err(ApplyError::Missing(revision));
                    }
                }
                Action::Thread { revision, action } => {
                    // TODO(cloudhead): Make sure we can deal with redacted revisions which are added
                    // to out of order, like in the `Merge` case.
                    if let Some(Redactable::Present(revision)) = self.revisions.get_mut(&revision) {
                        revision.discussion.apply(
                            [cob::Op::new(
                                op.id,
                                action,
                                op.author,
                                timestamp,
                                op.clock,
                                op.identity,
                            )],
                            repo,
                        )?;
                    } else {
                        return Err(ApplyError::Missing(revision));
                    }
                }
            }
        }
        Ok(())
    }
}

/// A patch revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// Author of the revision.
    author: Author,
    /// Revision description.
    description: LWWReg<Max<String>>,
    /// Base branch commit, used as a merge base.
    base: git::Oid,
    /// Reference to the Git object containing the code (revision head).
    oid: git::Oid,
    /// Discussion around this revision.
    discussion: Thread,
    /// Merges of this revision into other repositories.
    merges: LWWSet<Max<Merge>>,
    /// Reviews of this revision's changes (one per actor).
    reviews: GMap<ActorId, Review>,
    /// When this revision was created.
    timestamp: Timestamp,
}

impl Revision {
    pub fn new(
        author: Author,
        description: String,
        base: git::Oid,
        oid: git::Oid,
        timestamp: Timestamp,
        clock: Clock,
    ) -> Self {
        Self {
            author,
            description: LWWReg::new(Max::from(description), clock),
            base,
            oid,
            discussion: Thread::default(),
            merges: LWWSet::default(),
            reviews: GMap::default(),
            timestamp,
        }
    }

    pub fn description(&self) -> &str {
        self.description.get()
    }

    /// Author of the revision.
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// Base branch commit, used as a merge base.
    pub fn base(&self) -> &git::Oid {
        &self.base
    }

    /// Reference to the Git object containing the code (revision head).
    pub fn head(&self) -> git::Oid {
        self.oid
    }

    /// When this revision was created.
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Discussion around this revision.
    pub fn discussion(&self) -> &Thread {
        &self.discussion
    }

    /// Merges of this revision into other repositories.
    pub fn merges(&self) -> impl Iterator<Item = &Merge> {
        self.merges.iter().map(|m| m.get())
    }

    /// Reviews of this revision's changes (one per actor).
    pub fn reviews(&self) -> impl DoubleEndedIterator<Item = (&PublicKey, &Review)> {
        self.reviews.iter()
    }
}

/// Patch state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum State {
    #[default]
    Open,
    Draft,
    Archived,
    Merged,
}

impl FromStr for State {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "draft" => Ok(Self::Draft),
            "merged" => Ok(Self::Merged),
            "archived" => Ok(Self::Archived),
            _ => Err("Unrecognised patch state"),
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archived => write!(f, "archived"),
            Self::Draft => write!(f, "draft"),
            Self::Open => write!(f, "open"),
            Self::Merged => write!(f, "merged"),
        }
    }
}

/// A merged patch revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct CodeLocation {
    /// File being commented on.
    pub blob: git::Oid,
    /// Path of file being commented on.
    pub path: PathBuf,
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
        (
            &self.blob,
            &self.path,
            &self.commit,
            &self.lines.start,
            &self.lines.end,
        )
            .cmp(&(
                &other.blob,
                &other.path,
                &other.commit,
                &other.lines.start,
                &other.lines.end,
            ))
    }
}

/// Comment on code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeComment {
    /// Code location of the comment.
    location: CodeLocation,
    /// Comment.
    comment: String,
    /// Timestamp.
    timestamp: Timestamp,
}

impl CodeComment {
    /// Code location of the comment.
    pub fn location(&self) -> &CodeLocation {
        &self.location
    }

    /// Comment.
    pub fn comment(&self) -> &str {
        &self.comment
    }

    /// Timestamp.
    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }
}

/// A patch review on a revision.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Review {
    /// Review verdict.
    ///
    /// Nb. if the verdict is set and a subsequent review is made with
    /// the verdict as `None`, the original verdict will be nullified.
    verdict: LWWReg<Option<Verdict>>,
    /// Review general comment.
    ///
    /// Nb. if the comment is set and a subsequent review is made with
    /// the comment as `None`, the original comment will be nullified.
    comment: LWWReg<Option<Max<String>>>,
    /// Review inline code comments.
    inline: LWWSet<Max<CodeComment>>,
    /// Review timestamp.
    timestamp: Max<Timestamp>,
}

impl Serialize for Review {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Review", 4)?;
        state.serialize_field("verdict", &self.verdict())?;
        state.serialize_field("comment", &self.comment())?;
        state.serialize_field("inline", &self.inline().collect::<Vec<_>>())?;
        state.serialize_field("timestamp", &self.timestamp())?;
        state.end()
    }
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
        clock: Clock,
    ) -> Self {
        Self {
            verdict: LWWReg::new(verdict, clock),
            comment: LWWReg::new(comment.map(Max::from), clock),
            inline: LWWSet::from_iter(
                inline
                    .into_iter()
                    .map(Max::from)
                    .zip(std::iter::repeat(clock)),
            ),
            timestamp: Max::from(timestamp),
        }
    }

    /// Review verdict.
    pub fn verdict(&self) -> Option<Verdict> {
        self.verdict.get().as_ref().copied()
    }

    /// Review inline code comments.
    pub fn inline(&self) -> impl Iterator<Item = &CodeComment> {
        self.inline.iter().map(|m| m.get())
    }

    /// Review general comment.
    pub fn comment(&self) -> Option<&str> {
        self.comment.get().as_ref().map(|m| m.get().as_str())
    }

    /// Review timestamp.
    pub fn timestamp(&self) -> Timestamp {
        *self.timestamp.get()
    }
}

impl store::Transaction<Patch> {
    pub fn edit(
        &mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
    ) -> Result<(), store::Error> {
        self.push(Action::Edit {
            title: title.to_string(),
            description: description.to_string(),
            target,
        })
    }

    pub fn edit_revision(
        &mut self,
        revision: RevisionId,
        description: impl ToString,
    ) -> Result<(), store::Error> {
        self.push(Action::EditRevision {
            revision,
            description: description.to_string(),
        })
    }

    /// Start a patch revision discussion.
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

    /// Comment on a patch revision.
    pub fn comment<S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        reply_to: Option<CommentId>,
    ) -> Result<(), store::Error> {
        self.push(Action::Thread {
            revision,
            action: thread::Action::Comment {
                body: body.to_string(),
                reply_to,
            },
        })
    }

    /// Review a patch revision.
    pub fn review(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        comment: Option<String>,
        inline: Vec<CodeComment>,
    ) -> Result<(), store::Error> {
        self.push(Action::Review {
            revision,
            comment,
            verdict,
            inline,
        })
    }

    /// Merge a patch revision.
    pub fn merge(&mut self, revision: RevisionId, commit: git::Oid) -> Result<(), store::Error> {
        self.push(Action::Merge { revision, commit })
    }

    /// Update a patch with a new revision.
    pub fn revision(
        &mut self,
        description: impl ToString,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
    ) -> Result<(), store::Error> {
        self.push(Action::Revision {
            description: description.to_string(),
            base: base.into(),
            oid: oid.into(),
        })
    }

    /// Lifecycle a patch.
    pub fn lifecycle(&mut self, state: State) -> Result<(), store::Error> {
        self.push(Action::Lifecycle { state })
    }

    /// Tag a patch.
    pub fn tag(
        &mut self,
        add: impl IntoIterator<Item = Tag>,
        remove: impl IntoIterator<Item = Tag>,
    ) -> Result<(), store::Error> {
        let add = add.into_iter().collect::<Vec<_>>();
        let remove = remove.into_iter().collect::<Vec<_>>();

        self.push(Action::Tag { add, remove })
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

    pub fn transaction<G, F>(
        &mut self,
        message: &str,
        signer: &G,
        operations: F,
    ) -> Result<EntryId, Error>
    where
        G: Signer,
        F: FnOnce(&mut Transaction<Patch>) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::new(*signer.public_key(), self.clock);
        operations(&mut tx)?;
        let (ops, clock, commit) = tx.commit(message, self.id, &mut self.store.raw, signer)?;

        self.patch.apply(ops, self.store.as_ref())?;
        self.clock = clock;

        Ok(commit)
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
    ) -> Result<EntryId, Error> {
        self.transaction("Edit", signer, |tx| tx.edit(title, description, target))
    }

    /// Edit revision metadata.
    pub fn edit_revision<G: Signer>(
        &mut self,
        revision: RevisionId,
        description: String,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit Revision", signer, |tx| {
            tx.edit_revision(revision, description)
        })
    }

    /// Create a thread on a patch revision.
    pub fn thread<G: Signer, S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        signer: &G,
    ) -> Result<CommentId, Error> {
        self.transaction("Create thread", signer, |tx| tx.thread(revision, body))
    }

    /// Comment on a patch revision.
    pub fn comment<G: Signer, S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        reply_to: Option<CommentId>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Comment", signer, |tx| tx.comment(revision, body, reply_to))
    }

    /// Review a patch revision.
    pub fn review<G: Signer>(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        comment: Option<String>,
        inline: Vec<CodeComment>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Review", signer, |tx| {
            tx.review(revision, verdict, comment, inline)
        })
    }

    /// Merge a patch revision.
    pub fn merge<G: Signer>(
        &mut self,
        revision: RevisionId,
        commit: git::Oid,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Merge revision", signer, |tx| tx.merge(revision, commit))
    }

    /// Update a patch with a new revision.
    pub fn update<G: Signer>(
        &mut self,
        description: impl ToString,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Add revision", signer, |tx| {
            tx.revision(description, base, oid)
        })
    }

    /// Lifecycle a patch.
    pub fn lifecycle<G: Signer>(&mut self, state: State, signer: &G) -> Result<EntryId, Error> {
        self.transaction("Lifecycle", signer, |tx| tx.lifecycle(state))
    }

    /// Archive a patch.
    pub fn archive<G: Signer>(&mut self, signer: &G) -> Result<EntryId, Error> {
        self.lifecycle(State::Archived, signer)
    }

    /// Tag a patch.
    pub fn tag<G: Signer>(
        &mut self,
        add: impl IntoIterator<Item = Tag>,
        remove: impl IntoIterator<Item = Tag>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Tag", signer, |tx| tx.tag(add, remove))
    }
}

impl<'a, 'g> Deref for PatchMut<'a, 'g> {
    type Target = Patch;

    fn deref(&self) -> &Self::Target {
        &self.patch
    }
}

/// Detailed information on patch states
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCounts {
    pub open: usize,
    pub draft: usize,
    pub archived: usize,
    pub merged: usize,
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
    pub fn open(repository: &'a storage::Repository) -> Result<Self, store::Error> {
        let raw = store::Store::open(repository)?;

        Ok(Self { raw })
    }

    /// Create a patch.
    pub fn create<'g, G: Signer>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        tags: &[Tag],
        signer: &G,
    ) -> Result<PatchMut<'a, 'g>, Error> {
        let (id, patch, clock) =
            Transaction::initial("Create patch", &mut self.raw, signer, |tx| {
                tx.revision(String::default(), base, oid)?;
                tx.edit(title, description, target)?;
                tx.tag(tags.to_owned(), [])?;

                Ok(())
            })?;
        // Just a sanity check that our clock is advancing as expected.
        debug_assert_eq!(clock.get(), 1);

        Ok(PatchMut::new(id, patch, clock, self))
    }

    /// Patches count by state.
    pub fn counts(&self) -> Result<PatchCounts, store::Error> {
        let all = self.all()?;
        let state_groups =
            all.filter_map(|s| s.ok())
                .fold(PatchCounts::default(), |mut state, (_, p, _)| {
                    match p.state() {
                        State::Draft => state.draft += 1,
                        State::Open => state.open += 1,
                        State::Archived => state.archived += 1,
                        State::Merged => state.merged += 1,
                    }
                    state
                });

        Ok(state_groups)
    }

    /// Find the `Patch` containing the given `Revision`.
    pub fn find_by_revision(
        &self,
        id: &RevisionId,
    ) -> Result<Option<(PatchId, Patch, Revision)>, Error> {
        // Revision may be the patch's first, making it have the same ID.
        let p_id = ObjectId::from(id);
        if let Some(p) = self.get(&p_id)? {
            return Ok(p.revision(id).map(|r| (p_id, p.clone(), r.clone())));
        }

        let result = self
            .all()?
            .into_iter()
            .filter_map(|result| result.ok())
            .find_map(|(p_id, p, _)| p.revision(id).map(|r| (p_id, p.clone(), r.clone())));
        Ok(result)
    }

    /// Get a patch.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Patch>, store::Error> {
        self.raw.get(id).map(|r| r.map(|(p, _)| p))
    }

    /// Get a patch mutably.
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
    ) -> Result<impl Iterator<Item = (PatchId, Patch, clock::Lamport)> + 'a, Error> {
        let all = self.all()?;

        Ok(all
            .into_iter()
            .filter_map(|result| result.ok())
            .filter(|(_, p, _)| p.is_open()))
    }

    /// Get patches proposed by the given key.
    pub fn proposed_by<'b>(
        &'b self,
        who: &'b Did,
    ) -> Result<impl Iterator<Item = (PatchId, Patch, clock::Lamport)> + '_, Error> {
        Ok(self
            .proposed()?
            .filter(move |(_, p, _)| p.author().id() == who))
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::str::FromStr;
    use std::{array, iter};

    use radicle_crdt::test::{assert_laws, WeightedGenerator};

    use pretty_assertions::assert_eq;
    use qcheck::{Arbitrary, TestResult};

    use super::*;
    use crate::cob::test::Actor;
    use crate::crypto::test::signer::MockSigner;
    use crate::test;
    use crate::test::arbitrary::gen;
    use crate::test::storage::MockRepository;

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
        fn arbitrary(g: &mut qcheck::Gen) -> Self {
            type State = (
                Actor<MockSigner, Action>,
                clock::Lamport,
                Vec<EntryId>,
                Vec<Tag>,
            );

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

            let gen = WeightedGenerator::<(clock::Lamport, Op), State>::new(rng.clone())
                .variant(1, |(actor, clock, _, _), rng| {
                    Some((
                        clock.tick(),
                        actor.op(Action::Edit {
                            title: iter::repeat_with(|| rng.alphabetic()).take(8).collect(),
                            description: iter::repeat_with(|| rng.alphabetic()).take(16).collect(),
                            target: MergeTarget::Delegates,
                        }),
                    ))
                })
                .variant(1, |(actor, clock, revisions, _), rng| {
                    if revisions.is_empty() {
                        return None;
                    }
                    let revision = revisions[rng.usize(..revisions.len())];
                    let commit = oids[rng.usize(..oids.len())];

                    Some((clock.tick(), actor.op(Action::Merge { revision, commit })))
                })
                .variant(1, |(actor, clock, revisions, _), rng| {
                    if revisions.is_empty() {
                        return None;
                    }
                    let revision = revisions[rng.usize(..revisions.len())];

                    Some((clock.tick(), actor.op(Action::Redact { revision })))
                })
                .variant(1, |(actor, clock, _, tags), rng| {
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
                    Some((clock.tick(), actor.op(Action::Tag { add, remove })))
                })
                .variant(1, |(actor, clock, revisions, _), rng| {
                    let oid = oids[rng.usize(..oids.len())];
                    let base = oids[rng.usize(..oids.len())];
                    let description = iter::repeat_with(|| rng.alphabetic()).take(6).collect();
                    let op = actor.op(Action::Revision {
                        description,
                        base,
                        oid,
                    });

                    if rng.bool() {
                        revisions.push(op.id);
                    }
                    Some((*clock, op))
                });

            let mut changes = Vec::new();
            let mut permutations: [Vec<Op>; N] = array::from_fn(|_| Vec::new());

            for (_, op) in gen.take(g.size()) {
                changes.push(op);
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
        fn property(repo: MockRepository, log: Changes<3>) -> TestResult {
            let t = Patch::default();
            let [p1, p2, p3] = log.permutations;

            let mut t1 = t.clone();
            if t1.apply(p1, &repo).is_err() {
                return TestResult::discard();
            }

            let mut t2 = t.clone();
            if t2.apply(p2, &repo).is_err() {
                return TestResult::discard();
            }

            let mut t3 = t;
            if t3.apply(p3, &repo).is_err() {
                return TestResult::discard();
            }

            assert_eq!(t1, t2);
            assert_eq!(t2, t3);
            assert_laws(&t1, &t2, &t3);

            TestResult::passed()
        }

        qcheck::QuickCheck::new()
            .min_tests_passed(100)
            .gen(qcheck::Gen::new(7))
            .quickcheck(property as fn(MockRepository, Changes<3>) -> TestResult);
    }

    #[test]
    fn test_json_serialization() {
        let edit = Action::Tag {
            add: vec![],
            remove: vec![],
        };
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            String::from(r#"{"type":"tag","add":[],"remove":[]}"#)
        );
    }

    #[test]
    fn test_patch_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut patches = Patches::open(&project).unwrap();
        let author: Did = signer.public_key().into();
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

        assert_eq!(patch.clock.get(), 1);

        let patch_id = patch.id;
        let patch = patches.get(&patch_id).unwrap().unwrap();

        assert_eq!(patch.title(), "My first patch");
        assert_eq!(patch.description(), "Blah blah blah.");
        assert_eq!(patch.author().id(), &author);
        assert_eq!(patch.state(), State::Open);
        assert_eq!(patch.target(), target);
        assert_eq!(patch.version(), 0);

        let (rev_id, revision) = patch.latest().unwrap();

        assert_eq!(revision.author.id(), &author);
        assert_eq!(revision.description(), "");
        assert_eq!(revision.discussion.len(), 0);
        assert_eq!(revision.oid, oid);
        assert_eq!(revision.base, base);

        let (id, _, _) = patches.find_by_revision(rev_id).unwrap().unwrap();
        assert_eq!(id, patch_id);
    }

    #[test]
    fn test_patch_discussion() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let mut patches = Patches::open(&project).unwrap();
        let patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                git::Oid::try_from("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap(),
                git::Oid::try_from("e2a85016a458cd809c0ecee81f8c99613b0b0945").unwrap(),
                &[],
                &signer,
            )
            .unwrap();

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (revision_id, _) = patch.revisions().last().unwrap();
        assert!(
            patch
                .comment(*revision_id, "patch comment", None, &signer)
                .is_ok(),
            "can comment on patch"
        );

        let (_, revision) = patch.revisions().last().unwrap();
        let (_, comment) = revision.discussion.first().unwrap();
        assert_eq!("patch comment", comment.body(), "comment body untouched");
    }

    #[test]
    fn test_patch_merge() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let oid = git::Oid::from_str("e2a85016a458cd809c0ecee81f8c99613b0b0945").unwrap();
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let mut patches = Patches::open(&project).unwrap();
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
        let mut patches = Patches::open(&project).unwrap();
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
        let repo = gen::<MockRepository>(1);

        let a1 = alice.op(Action::Revision {
            description: String::new(),
            base,
            oid,
        });
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

        patch.apply([a1], &repo).unwrap();
        assert!(patch.revisions().next().is_some());

        patch.apply([a2], &repo).unwrap();
        assert!(patch.revisions().next().is_none());

        patch.apply([a3], &repo).unwrap_err();
        patch.apply([a4], &repo).unwrap_err();
    }

    #[test]
    fn test_revision_redact_reinsert() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<_, Action>::new(MockSigner::default());
        let mut p1 = Patch::default();
        let mut p2 = Patch::default();

        let a1 = alice.op(Action::Revision {
            description: String::new(),
            base,
            oid,
        });
        let a2 = alice.op(Action::Redact { revision: a1.id() });

        p1.apply([a1.clone(), a2.clone(), a1.clone()], &repo)
            .unwrap();
        p2.apply([a1.clone(), a1, a2], &repo).unwrap();

        assert_eq!(p1, p2);
    }

    #[test]
    fn test_revision_merge_reinsert() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let repo = gen::<MockRepository>(1);
        let mut alice = Actor::<_, Action>::new(MockSigner::default());
        let mut p1 = Patch::default();
        let mut p2 = Patch::default();

        let a1 = alice.op(Action::Revision {
            description: String::new(),
            base,
            oid,
        });
        let a2 = alice.op(Action::Merge {
            revision: a1.id(),
            commit: oid,
        });

        p1.apply([a1.clone(), a2.clone(), a1.clone()], &repo)
            .unwrap();
        p2.apply([a1.clone(), a1, a2], &repo).unwrap();

        assert_eq!(p1, p2);
    }

    #[test]
    fn test_patch_review_edit() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let blob = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d133999").unwrap();
        let mut patches = Patches::open(&project).unwrap();
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

        let inline = vec![CodeComment {
            location: CodeLocation {
                blob,
                path: Path::new("file.rs").to_path_buf(),
                commit: oid,
                lines: 1..3,
            },
            comment: "Nice!".to_owned(),
            timestamp: Timestamp::new(0),
        }];
        patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                inline.clone(),
                &signer,
            )
            .unwrap();
        patch
            .review(
                rid,
                Some(Verdict::Reject),
                Some("LGTM".to_owned()),
                vec![],
                &signer,
            )
            .unwrap(); // Overwrite the verdict.

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest().unwrap();
        assert_eq!(revision.reviews.len(), 1, "the reviews were merged");

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.comment(), Some("LGTM"));
        assert_eq!(review.inline().cloned().collect::<Vec<_>>(), inline);

        patch
            .review(
                rid,
                Some(Verdict::Reject),
                Some("Whoops!".to_owned()),
                vec![],
                &signer,
            )
            .unwrap(); // Overwrite the comment.
        let (_, revision) = patch.latest().unwrap();
        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.comment(), Some("Whoops!"));
        assert_eq!(review.inline().cloned().collect::<Vec<_>>(), inline);
    }

    #[test]
    fn test_patch_reject_to_accept() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut patches = Patches::open(&project).unwrap();
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
                Some(Verdict::Reject),
                Some("Nah".to_owned()),
                vec![],
                &signer,
            )
            .unwrap();
        patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                &signer,
            )
            .unwrap();

        let id = patch.id;
        let patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest().unwrap();
        assert_eq!(revision.reviews.len(), 1, "the reviews were merged");

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.comment(), Some("LGTM"));
    }

    #[test]
    fn test_patch_review_remove_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut patches = Patches::open(&project).unwrap();
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
                Some(Verdict::Reject),
                Some("Nah".to_owned()),
                vec![],
                &signer,
            )
            .unwrap();
        patch.review(rid, None, None, vec![], &signer).unwrap();

        let id = patch.id;
        let patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest().unwrap();

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), None);
        assert_eq!(review.comment(), None);
    }

    #[test]
    fn test_patch_update() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let base = git::Oid::from_str("af08e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let rev0_oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let rev1_oid = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let mut patches = Patches::open(&project).unwrap();
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

        assert_eq!(patch.clock.get(), 1);
        assert_eq!(patch.description(), "Blah blah blah.");
        assert_eq!(patch.version(), 0);

        let _ = patch
            .update("I've made changes.", base, rev1_oid, &signer)
            .unwrap();
        assert_eq!(patch.clock.get(), 2);

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();
        assert_eq!(patch.version(), 1);
        assert_eq!(patch.revisions.len(), 2);
        assert_eq!(patch.revisions().count(), 2);
        assert_eq!(patch.revisions().nth(0).unwrap().1.description(), "");
        assert_eq!(
            patch.revisions().nth(1).unwrap().1.description(),
            "I've made changes."
        );

        let (_, revision) = patch.latest().unwrap();

        assert_eq!(patch.version(), 1);
        assert_eq!(revision.oid, rev1_oid);
        assert_eq!(revision.description(), "I've made changes.");
    }
}
