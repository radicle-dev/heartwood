#![allow(clippy::too_many_arguments)]
use std::collections::HashMap;
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
use radicle_crdt::{GMap, GSet, LWWMap, LWWReg, LWWSet, Lamport, Max, Redactable, Semilattice};

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
use crate::identity;
use crate::identity::doc::DocError;
use crate::identity::PayloadError;
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
pub enum Error {
    /// Error trying to delete the protected root revision.
    #[error("refusing to delete root revision: {0}")]
    RootRevision(RevisionId),
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
    Thread(#[from] thread::Error),
    /// Error loading the identity document committed to by an operation.
    #[error("identity doc failed to load: {0}")]
    Doc(#[from] DocError),
    /// Error loading the document payload.
    #[error("payload failed to load: {0}")]
    Payload(#[from] PayloadError),
    /// The merge operation is invalid.
    #[error("invalid merge operation in {0}")]
    InvalidMerge(EntryId),
    /// Git error.
    #[error("git: {0}")]
    Git(#[from] git::ext::Error),
    /// Validation error.
    #[error("validation failed: {0}")]
    Validate(&'static str),
    /// Store error.
    #[error("store: {0}")]
    Store(#[from] store::Error),
}

/// Patch operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Edit {
        title: String,
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

impl MergeTarget {
    /// Get the head of the target branch.
    pub fn head<R: ReadRepository>(&self, repo: &R) -> Result<git::Oid, identity::IdentityError> {
        match self {
            MergeTarget::Delegates => {
                let (_, target) = repo.head()?;
                Ok(target)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// Title of the patch.
    title: LWWReg<Max<String>>,
    /// Current state of the patch.
    state: LWWReg<Max<State>>,
    /// Target this patch is meant to be merged in.
    target: LWWReg<Max<MergeTarget>>,
    /// Associated tags.
    tags: LWWSet<Tag>,
    /// Patch merges.
    merges: LWWMap<ActorId, Redactable<Merge>>,
    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    revisions: GMap<RevisionId, Redactable<Revision>>,
    /// Users assigned to review this patch.
    reviewers: LWWSet<ActorId>,
    /// Timeline of operations.
    timeline: GSet<(Lamport, EntryId)>,
}

impl Semilattice for Patch {
    fn merge(&mut self, other: Self) {
        self.title.merge(other.title);
        self.state.merge(other.state);
        self.target.merge(other.target);
        self.merges.merge(other.merges);
        self.tags.merge(other.tags);
        self.revisions.merge(other.revisions);
        self.reviewers.merge(other.reviewers);
        self.timeline.merge(other.timeline);
    }
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            title: LWWReg::initial(Max::from(String::default())),
            state: LWWReg::initial(Max::from(State::default())),
            target: LWWReg::initial(Max::from(MergeTarget::default())),
            tags: LWWSet::default(),
            merges: LWWMap::default(),
            revisions: GMap::default(),
            reviewers: LWWSet::default(),
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
    pub fn state(&self) -> &State {
        self.state.get().get()
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
        self.root().description()
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

    /// List of patch reviewers.
    pub fn reviewers(&self) -> impl Iterator<Item = Did> + '_ {
        self.reviewers.iter().map(Did::from)
    }

    /// Get the merges.
    pub fn merges(&self) -> impl Iterator<Item = (&ActorId, &Merge)> {
        self.merges
            .iter()
            .filter_map(|(a, m)| m.get().map(|m| (a, m)))
    }

    /// Reference to the Git object containing the code on the latest revision.
    pub fn head(&self) -> &git::Oid {
        &self.latest().1.oid
    }

    /// Get the commit of the target branch on which this patch is based.
    /// This can change via a patch update.
    pub fn base(&self) -> &git::Oid {
        &self.latest().1.base
    }

    /// Get the merge base of this patch.
    pub fn merge_base<R: ReadRepository>(&self, repo: &R) -> Result<git::Oid, git::ext::Error> {
        repo.merge_base(self.base(), self.head())
    }

    /// Get the commit range of this patch.
    pub fn range<R: ReadRepository>(
        &self,
        repo: &R,
    ) -> Result<(git::Oid, git::Oid), git::ext::Error> {
        if self.is_merged() {
            Ok((*self.base(), *self.head()))
        } else {
            Ok((self.merge_base(repo)?, *self.head()))
        }
    }

    /// Index of latest revision in the revisions list.
    pub fn version(&self) -> RevisionIx {
        self.revisions
            .len()
            .checked_sub(1)
            .expect("Patch::version: at least one revision is present")
    }

    /// Root revision.
    ///
    /// This is the revision that was created with the patch.
    pub fn root(&self) -> &Revision {
        self.revisions()
            .next()
            .map(|(_, r)| r)
            .expect("Patch::root: there is always a root revision")
    }

    /// Latest revision.
    pub fn latest(&self) -> (&RevisionId, &Revision) {
        self.revisions()
            .next_back()
            .expect("Patch::latest: there is always at least one revision")
    }

    /// Time of last update.
    pub fn updated_at(&self) -> Timestamp {
        self.latest().1.timestamp()
    }

    /// Check if the patch is merged.
    pub fn is_merged(&self) -> bool {
        matches!(self.state(), State::Merged { .. })
    }

    /// Check if the patch is open.
    pub fn is_open(&self) -> bool {
        matches!(self.state(), State::Open { .. })
    }

    /// Check if the patch is archived.
    pub fn is_archived(&self) -> bool {
        matches!(self.state(), State::Archived)
    }

    /// Check if the patch is a draft.
    pub fn is_draft(&self) -> bool {
        matches!(self.state(), State::Draft)
    }
}

impl store::FromHistory for Patch {
    type Action = Action;
    type Error = Error;

    fn type_name() -> &'static TypeName {
        &*TYPENAME
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.revisions.is_empty() {
            return Err(Error::Validate("no revisions found"));
        }
        if self.title().is_empty() {
            return Err(Error::Validate("empty title"));
        }
        Ok(())
    }

    fn apply<R: ReadRepository>(
        &mut self,
        ops: impl IntoIterator<Item = Op>,
        repo: &R,
    ) -> Result<(), Error> {
        for op in ops {
            let id = op.id;
            let author = Author::new(op.author);
            let timestamp = op.timestamp;

            self.timeline.insert((op.clock, id));

            match op.action {
                Action::Edit { title, target } => {
                    self.title.set(title, op.clock);
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
                        return Err(Error::Missing(revision));
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
                        return Err(Error::Missing(revision));
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
                        return Err(Error::Missing(revision));
                    }
                }
                Action::Merge { revision, commit } => {
                    if let Some(Redactable::Present(_)) = self.revisions.get_mut(&revision) {
                        let doc = repo.identity_doc_at(op.identity)?.verified()?;

                        match self.target() {
                            MergeTarget::Delegates => {
                                if !doc.is_delegate(&op.author) {
                                    return Err(Error::InvalidMerge(op.id));
                                }
                                let proj = doc.project()?;
                                let branch = git::refs::branch(proj.default_branch());

                                // Nb. We don't return an error in case the merge commit is not an
                                // ancestor of the default branch. The default branch can change
                                // *after* the merge action is created, which is out of the control
                                // of the merge author. We simply skip it, which allows archiving in
                                // case of a rebase off the master branch, or a redaction of the
                                // merge.
                                let Ok(head) = repo.reference_oid(&op.author, &branch) else {
                                    continue;
                                };
                                if commit != head && !repo.is_ancestor_of(commit, head)? {
                                    continue;
                                }
                            }
                        }
                        self.merges.insert(
                            op.author,
                            Redactable::Present(Merge {
                                revision,
                                commit,
                                timestamp,
                            }),
                            op.clock,
                        );

                        let mut merges = self.merges.iter().fold(
                            HashMap::<(RevisionId, git::Oid), usize>::new(),
                            |mut acc, (_, merge)| {
                                if let Some(merge) = merge.get() {
                                    *acc.entry((merge.revision, merge.commit)).or_default() += 1;
                                }
                                acc
                            },
                        );
                        // Discard revisions that weren't merged by a threshold of delegates.
                        merges.retain(|_, count| *count >= doc.threshold);

                        match merges.into_keys().collect::<Vec<_>>().as_slice() {
                            [] => {
                                // None of the revisions met the quorum.
                            }
                            [(revision, commit)] => {
                                // Patch is merged.
                                self.state.set(
                                    State::Merged {
                                        revision: *revision,
                                        commit: *commit,
                                    },
                                    op.clock,
                                );
                            }
                            revisions => {
                                // More than one revision met the quorum.
                                self.state.set(
                                    State::Open {
                                        conflicts: revisions.to_vec(),
                                    },
                                    op.clock,
                                );
                            }
                        }
                    } else {
                        return Err(Error::Missing(revision));
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
                        return Err(Error::Missing(revision));
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

    /// Reviews of this revision's changes (one per actor).
    pub fn reviews(&self) -> impl DoubleEndedIterator<Item = (&PublicKey, &Review)> {
        self.reviews.iter()
    }
}

/// Patch state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum State {
    Draft,
    Open {
        /// Revisions that were merged and are conflicting.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[serde(default)]
        conflicts: Vec<(RevisionId, git::Oid)>,
    },
    Archived,
    Merged {
        /// The revision that was merged.
        revision: RevisionId,
        /// The commit in the target branch that contains the changes.
        commit: git::Oid,
    },
}

impl Default for State {
    fn default() -> Self {
        Self::Open { conflicts: vec![] }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archived => write!(f, "archived"),
            Self::Draft => write!(f, "draft"),
            Self::Open { .. } => write!(f, "open"),
            Self::Merged { .. } => write!(f, "merged"),
        }
    }
}

/// A merged patch revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct Merge {
    /// Revision that was merged.
    pub revision: RevisionId,
    /// Base branch commit that contains the revision.
    pub commit: git::Oid,
    /// When this merge was performed.
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
    pub fn edit(&mut self, title: impl ToString, target: MergeTarget) -> Result<(), store::Error> {
        self.push(Action::Edit {
            title: title.to_string(),
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

    /// Redact the revision.
    pub fn redact(&mut self, revision: RevisionId) -> Result<(), store::Error> {
        self.push(Action::Redact { revision })
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
        target: MergeTarget,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit", signer, |tx| tx.edit(title, target))
    }

    /// Edit revision metadata.
    pub fn edit_revision<G: Signer>(
        &mut self,
        revision: RevisionId,
        description: String,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit revision", signer, |tx| {
            tx.edit_revision(revision, description)
        })
    }

    /// Redact a revision.
    pub fn redact<G: Signer>(
        &mut self,
        revision: RevisionId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        if revision == RevisionId::from(self.id) {
            return Err(Error::RootRevision(revision));
        }
        self.transaction("Redact revision", signer, |tx| tx.redact(revision))
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
        // TODO: Don't allow merging the same revision twice?
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

    /// Mark a patch as ready to be reviewed. Returns `false` if the patch was not a draft.
    pub fn ready<G: Signer>(&mut self, signer: &G) -> Result<bool, Error> {
        if !self.is_draft() {
            return Ok(false);
        }
        self.lifecycle(State::Open { conflicts: vec![] }, signer)?;

        Ok(true)
    }

    /// Mark an open patch as a draft. Returns `false` if the patch was not open and free of merges.
    pub fn unready<G: Signer>(&mut self, signer: &G) -> Result<bool, Error> {
        if !matches!(self.state(), State::Open { conflicts } if conflicts.is_empty()) {
            return Ok(false);
        }
        self.lifecycle(State::Draft, signer)?;

        Ok(true)
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

    /// Open a new patch.
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
        self._create(
            title,
            description,
            target,
            base,
            oid,
            tags,
            State::default(),
            signer,
        )
    }

    /// Draft a patch. This patch will be created in a [`State::Draft`] state.
    pub fn draft<'g, G: Signer>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        tags: &[Tag],
        signer: &G,
    ) -> Result<PatchMut<'a, 'g>, Error> {
        self._create(
            title,
            description,
            target,
            base,
            oid,
            tags,
            State::Draft,
            signer,
        )
    }

    /// Patches count by state.
    pub fn counts(&self) -> Result<PatchCounts, store::Error> {
        let all = self.all()?;
        let state_groups =
            all.filter_map(|s| s.ok())
                .fold(PatchCounts::default(), |mut state, (_, p, _)| {
                    match p.state() {
                        State::Draft => state.draft += 1,
                        State::Open { .. } => state.open += 1,
                        State::Archived => state.archived += 1,
                        State::Merged { .. } => state.merged += 1,
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

    /// Create a patch. This is an internal function used by `create` and `draft`.
    fn _create<'g, G: Signer>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        tags: &[Tag],
        state: State,
        signer: &G,
    ) -> Result<PatchMut<'a, 'g>, Error> {
        let (id, patch, clock) =
            Transaction::initial("Create patch", &mut self.raw, signer, |tx| {
                tx.revision(description, base, oid)?;
                tx.edit(title, target)?;
                tx.tag(tags.to_owned(), [])?;

                if state != State::default() {
                    tx.lifecycle(state)?;
                }
                Ok(())
            })?;
        // Just a sanity check that our clock is advancing as expected.
        debug_assert_eq!(clock.get(), 1);

        Ok(PatchMut::new(id, patch, clock, self))
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::str::FromStr;
    use std::{array, iter};

    use radicle_crdt::test::{assert_laws, WeightedGenerator};

    use nonempty::nonempty;
    use pretty_assertions::assert_eq;
    use qcheck::{Arbitrary, TestResult};

    use super::*;
    use crate::assert_matches;
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
            type State = (Actor<MockSigner>, clock::Lamport, Vec<EntryId>, Vec<Tag>);

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
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let author: Did = signer.public_key().into();
        let target = MergeTarget::Delegates;
        let patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                target,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        assert_eq!(patch.clock.get(), 1);

        let patch_id = patch.id;
        let patch = patches.get(&patch_id).unwrap().unwrap();

        assert_eq!(patch.title(), "My first patch");
        assert_eq!(patch.description(), "Blah blah blah.");
        assert_eq!(patch.author().id(), &author);
        assert_eq!(patch.state(), &State::Open { conflicts: vec![] });
        assert_eq!(patch.target(), target);
        assert_eq!(patch.version(), 0);

        let (rev_id, revision) = patch.latest();

        assert_eq!(revision.author.id(), &author);
        assert_eq!(revision.description(), "Blah blah blah.");
        assert_eq!(revision.discussion.len(), 0);
        assert_eq!(revision.oid, pr.oid);
        assert_eq!(revision.base, pr.base);

        let (id, _, _) = patches.find_by_revision(rev_id).unwrap().unwrap();
        assert_eq!(id, patch_id);
    }

    #[test]
    fn test_patch_discussion() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (revision_id, _) = patch.revisions().last().unwrap();
        assert!(
            patch
                .comment(*revision_id, "patch comment", None, signer)
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
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        let id = patch.id;
        let (rid, _) = patch.revisions().next().unwrap();
        let _merge = patch.merge(*rid, pr.base, signer).unwrap();

        let patch = patches.get(&id).unwrap().unwrap();

        let merges = patch.merges.iter().collect::<Vec<_>>();
        assert_eq!(merges.len(), 1);

        let (merger, merge) = merges.first().unwrap();
        assert_eq!(*merger, signer.public_key());
        assert_eq!(merge.get().unwrap().commit, pr.base);
    }

    #[test]
    fn test_patch_merge_and_archive() {
        let rid = gen::<Id>(1);
        let base = git::Oid::from_str("d8711a8d43dc919fe39ae4b7c2f7b24667f5d470").unwrap();
        let commit = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();

        let mut alice = Actor::<MockSigner>::default();
        let mut bob = Actor::<MockSigner>::default();

        let proj = gen::<Project>(1);
        let doc = Doc::new(proj, nonempty![alice.did(), bob.did()], 1)
            .verified()
            .unwrap();
        let repo = MockRepository::new(rid, doc);
        let patch = alice
            .patch("Some changes", "", base, commit, &repo)
            .unwrap();
        let (revision, _) = patch.revisions().next().unwrap();

        // Create two concurrent operations.
        let clock = Lamport::from(2);
        let identity = repo.identity_head().unwrap();
        let ops = [
            alice.op_with(
                Action::Merge {
                    revision: *revision,
                    commit,
                },
                clock,
                identity,
            ),
            bob.op_with(
                Action::Lifecycle {
                    state: State::Archived,
                },
                clock,
                identity,
            ),
        ];

        let mut patch1 = patch.clone();
        let mut patch2 = patch.clone();

        // Apply the ops in different orders and expect the patch state to remain the same.
        patch1.apply(ops.iter().cloned(), &repo).unwrap();
        patch2.apply(ops.iter().cloned().rev(), &repo).unwrap();

        assert_matches!(patch1.state(), &State::Merged { .. });
        assert_matches!(patch2.state(), &State::Merged { .. });
    }

    #[test]
    fn test_patch_review() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        patch
            .review(
                *rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                signer,
            )
            .unwrap();

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();
        let (_, revision) = patch.latest();
        assert_eq!(revision.reviews.len(), 1);

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.comment(), Some("LGTM"));
    }

    #[test]
    fn test_revision_redacted() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut alice = Actor::new(MockSigner::default());
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
        let mut alice = Actor::new(MockSigner::default());
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
        let id = gen::<Id>(1);
        let mut alice = Actor::new(MockSigner::default());
        let mut doc = gen::<Doc<Verified>>(1);
        doc.delegates.push(alice.signer.public_key().into());
        let repo = MockRepository::new(id, doc);

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
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let blob = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d133999").unwrap();
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        let rid = *rid;

        let inline = vec![CodeComment {
            location: CodeLocation {
                blob,
                path: Path::new("file.rs").to_path_buf(),
                commit: pr.oid,
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
                signer,
            )
            .unwrap();
        patch
            .review(
                rid,
                Some(Verdict::Reject),
                Some("LGTM".to_owned()),
                vec![],
                signer,
            )
            .unwrap(); // Overwrite the verdict.

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest();
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
                signer,
            )
            .unwrap(); // Overwrite the comment.
        let (_, revision) = patch.latest();
        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.comment(), Some("Whoops!"));
        assert_eq!(review.inline().cloned().collect::<Vec<_>>(), inline);
    }

    #[test]
    fn test_patch_reject_to_accept() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        let rid = *rid;

        patch
            .review(
                rid,
                Some(Verdict::Reject),
                Some("Nah".to_owned()),
                vec![],
                signer,
            )
            .unwrap();
        patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                signer,
            )
            .unwrap();

        let id = patch.id;
        let patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest();
        assert_eq!(revision.reviews.len(), 1, "the reviews were merged");

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.comment(), Some("LGTM"));
    }

    #[test]
    fn test_patch_review_remove_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        let rid = *rid;

        patch
            .review(
                rid,
                Some(Verdict::Reject),
                Some("Nah".to_owned()),
                vec![],
                signer,
            )
            .unwrap();
        patch.review(rid, None, None, vec![], signer).unwrap();

        let id = patch.id;
        let patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest();

        let review = revision.reviews.get(signer.public_key()).unwrap();
        assert_eq!(review.verdict(), None);
        assert_eq!(review.comment(), None);
    }

    #[test]
    fn test_patch_update() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();

        assert_eq!(patch.clock.get(), 1);
        assert_eq!(patch.description(), "Blah blah blah.");
        assert_eq!(patch.version(), 0);

        let update = ctx.branch_with(test::setup::update_blobs());
        let _ = patch
            .update("I've made changes.", pr.base, update.oid, signer)
            .unwrap();
        assert_eq!(patch.clock.get(), 2);

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();
        assert_eq!(patch.version(), 1);
        assert_eq!(patch.revisions.len(), 2);
        assert_eq!(patch.revisions().count(), 2);
        assert_eq!(
            patch.revisions().nth(0).unwrap().1.description(),
            "Blah blah blah."
        );
        assert_eq!(
            patch.revisions().nth(1).unwrap().1.description(),
            "I've made changes."
        );

        let (_, revision) = patch.latest();

        assert_eq!(patch.version(), 1);
        assert_eq!(revision.oid, update.oid);
        assert_eq!(revision.description(), "I've made changes.");
    }

    #[test]
    fn test_patch_redact() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::setup::Context::new(&tmp);
        let signer = &ctx.signer;
        let pr = ctx.branch_with(test::setup::initial_blobs());
        let mut patches = Patches::open(&ctx.project).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                pr.base,
                pr.oid,
                &[],
                signer,
            )
            .unwrap();
        let patch_id = patch.id;

        let update = ctx.branch_with(test::setup::update_blobs());
        let revision_id = patch
            .update("I've made changes.", pr.base, update.oid, signer)
            .unwrap();
        assert_eq!(patch.revisions().count(), 2);

        patch.redact(revision_id, signer).unwrap();
        assert_eq!(patch.latest().0, &RevisionId::from(patch_id));
        assert_eq!(patch.revisions().count(), 1);

        // The patch's root must always exist.
        assert!(patch.redact(*patch.latest().0, signer).is_err());
    }
}
