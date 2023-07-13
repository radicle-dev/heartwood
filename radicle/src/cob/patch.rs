#![allow(clippy::too_many_arguments)]
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::ops::Deref;
use std::ops::Range;
use std::path::PathBuf;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    EditReview {
        review: EntryId,
        summary: Option<String>,
    },
    EditCodeComment {
        review: EntryId,
        comment: EntryId,
        body: String,
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
        summary: Option<String>,
        verdict: Option<Verdict>,
    },
    CodeComment {
        review: EntryId,
        body: String,
        location: CodeLocation,
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

/// Patch state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patch {
    /// Title of the patch.
    title: String,
    /// Current state of the patch.
    state: State,
    /// Target this patch is meant to be merged in.
    target: MergeTarget,
    /// Associated tags.
    /// Tags can be added and removed at will.
    tags: BTreeSet<Tag>,
    /// Patch merges.
    ///
    /// Only one merge is allowed per user.
    ///
    /// Merges can be removed and replaced, but not modified. Generally, once a revision is merged,
    /// it stays that way. Being able to remove merges may be useful in case of force updates
    /// on the target branch.
    merges: BTreeMap<ActorId, Merge>,
    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    ///
    /// Revisions can be redacted, but are otherwise immutable.
    revisions: BTreeMap<RevisionId, Option<Revision>>,
    /// Users assigned to review this patch.
    reviewers: BTreeSet<ActorId>,
    /// Timeline of operations.
    timeline: Vec<EntryId>,
    /// Reviews index. Keeps track of reviews for better performance.
    reviews: BTreeMap<EntryId, Option<(EntryId, ActorId)>>,
}

impl Patch {
    /// Title of the patch.
    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    /// Current state of the patch.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Target this patch is meant to be merged in.
    pub fn target(&self) -> MergeTarget {
        self.target
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
        self.revisions.get(id).and_then(|o| o.as_ref())
    }

    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = (&RevisionId, &Revision)> {
        self.timeline.iter().filter_map(|id| {
            self.revisions
                .get(id)
                .and_then(|o| o.as_ref())
                .map(|rev| (id, rev))
        })
    }

    /// List of patch reviewers.
    pub fn reviewers(&self) -> impl Iterator<Item = Did> + '_ {
        self.reviewers.iter().map(Did::from)
    }

    /// Get the merges.
    pub fn merges(&self) -> impl Iterator<Item = (&ActorId, &Merge)> {
        self.merges.iter().map(|(a, m)| (a, m))
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

    fn apply<R: ReadRepository>(&mut self, op: Op, repo: &R) -> Result<(), Error> {
        let id = op.id;
        let author = Author::new(op.author);
        let timestamp = op.timestamp;

        debug_assert!(!self.timeline.contains(&op.id));

        self.timeline.push(op.id);

        for action in op.actions {
            match action {
                Action::Edit { title, target } => {
                    self.title = title;
                    self.target = target;
                }
                Action::Lifecycle { state } => {
                    self.state = state;
                }
                Action::Tag { add, remove } => {
                    for tag in add {
                        self.tags.insert(tag);
                    }
                    for tag in remove {
                        self.tags.remove(&tag);
                    }
                }
                Action::EditRevision {
                    revision,
                    description,
                } => {
                    if let Some(redactable) = self.revisions.get_mut(&revision) {
                        // If the revision was redacted concurrently, there's nothing to do.
                        if let Some(revision) = redactable {
                            revision.description = description;
                        }
                    } else {
                        return Err(Error::Missing(revision));
                    }
                }
                Action::EditReview { review, summary } => {
                    let Some(Some((revision, author))) =
                        self.reviews.get(&review) else {
                            return Err(Error::Missing(review));
                    };
                    let Some(rev) = self.revisions.get_mut(revision) else {
                        return Err(Error::Missing(*revision));
                    };
                    // If the revision was redacted concurrently, there's nothing to do.
                    // Likewise, if the review was redacted concurrently, there's nothing to do.
                    if let Some(rev) = rev {
                        let Some(review) = rev.reviews.get_mut(author) else {
                            return Err(Error::Missing(review));
                        };
                        if let Some(review) = review {
                            review.summary = summary;
                        }
                    }
                }
                Action::Revision {
                    description,
                    base,
                    oid,
                } => {
                    self.revisions.insert(
                        id,
                        Some(Revision::new(
                            author.clone(),
                            description,
                            base,
                            oid,
                            timestamp,
                        )),
                    );
                }
                Action::Redact { revision } => {
                    // Redactions must have observed a revision to be valid.
                    if let Some(revision) = self.revisions.get_mut(&revision) {
                        *revision = None;
                    } else {
                        return Err(Error::Missing(revision));
                    }
                }
                Action::Review {
                    revision,
                    ref summary,
                    verdict,
                } => {
                    let Some(rev) = self.revisions.get_mut(&revision) else {
                        return Err(Error::Missing(revision));
                    };
                    if let Some(rev) = rev {
                        // Nb. Applying two reviews by the same author is not allowed and
                        // results in the review being redacted.
                        rev.reviews.insert(
                            op.author,
                            Some(Review::new(verdict, summary.to_owned(), timestamp)),
                        );
                        // Update reviews index.
                        self.reviews.insert(op.id, Some((revision, op.author)));
                    }
                }
                Action::EditCodeComment {
                    review,
                    comment,
                    body,
                } => {
                    match self.reviews.get(&review) {
                        Some(Some((revision, author))) => {
                            let Some(rev) = self.revisions.get_mut(revision) else {
                                return Err(Error::Missing(*revision));
                            };
                            // If the revision was redacted concurrently, there's nothing to do.
                            // Likewise, if the review was redacted concurrently, there's nothing to do.
                            if let Some(rev) = rev {
                                let Some(review) = rev.reviews.get_mut(author) else {
                                    return Err(Error::Missing(review));
                                };
                                if let Some(review) = review {
                                    let Some(comment) = review.comments.get_mut(&comment) else {
                                        return Err(Error::Missing(comment));
                                    };
                                    if let Some(comment) = comment {
                                        comment.edit(body, timestamp);
                                    }
                                }
                            }
                        }
                        Some(None) => {
                            // Redacted.
                        }
                        None => return Err(Error::Missing(review)),
                    }
                }
                Action::CodeComment {
                    review,
                    body,
                    location,
                } => {
                    match self.reviews.get(&review) {
                        Some(Some((revision, author))) => {
                            let Some(rev) = self.revisions.get_mut(revision) else {
                                return Err(Error::Missing(*revision));
                            };
                            // If the revision was redacted concurrently, there's nothing to do.
                            // Likewise, if the review was redacted concurrently, there's nothing to do.
                            if let Some(rev) = rev {
                                let Some(review) = rev.reviews.get_mut(author) else {
                                    return Err(Error::Missing(review));
                                };
                                if let Some(review) = review {
                                    review.comments.insert(
                                        id,
                                        Some(CodeComment::new(
                                            op.author, body, location, timestamp,
                                        )),
                                    );
                                }
                            }
                        }
                        Some(None) => {
                            // Redacted.
                        }
                        None => return Err(Error::Missing(review)),
                    }
                }
                Action::Merge { revision, commit } => {
                    let Some(rev) = self.revisions.get_mut(&revision) else {
                        return Err(Error::Missing(revision));
                    };
                    if rev.is_some() {
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
                            Merge {
                                revision,
                                commit,
                                timestamp,
                            },
                        );

                        let mut merges = self.merges.iter().fold(
                            HashMap::<(RevisionId, git::Oid), usize>::new(),
                            |mut acc, (_, merge)| {
                                *acc.entry((merge.revision, merge.commit)).or_default() += 1;
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
                                self.state = State::Merged {
                                    revision: *revision,
                                    commit: *commit,
                                };
                            }
                            revisions => {
                                // More than one revision met the quorum.
                                self.state = State::Open {
                                    conflicts: revisions.to_vec(),
                                };
                            }
                        }
                    }
                }
                Action::Thread { revision, action } => {
                    match self.revisions.get_mut(&revision) {
                        Some(Some(revision)) => {
                            revision.discussion.apply(
                                cob::Op::new(op.id, action, op.author, timestamp, op.identity),
                                repo,
                            )?;
                        }
                        Some(None) => {
                            // Redacted.
                        }
                        None => return Err(Error::Missing(revision)),
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
    description: String,
    /// Base branch commit, used as a merge base.
    base: git::Oid,
    /// Reference to the Git object containing the code (revision head).
    oid: git::Oid,
    /// Discussion around this revision.
    discussion: Thread,
    /// Reviews of this revision's changes (one per actor).
    reviews: BTreeMap<ActorId, Option<Review>>,
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
    ) -> Self {
        Self {
            author,
            description,
            base,
            oid,
            discussion: Thread::default(),
            reviews: BTreeMap::default(),
            timestamp,
        }
    }

    pub fn description(&self) -> &str {
        self.description.as_str()
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
        self.reviews
            .iter()
            .filter_map(|(author, review)| review.as_ref().map(|r| (author, r)))
    }

    /// Get a review by author.
    pub fn review(&self, author: &ActorId) -> Option<&Review> {
        self.reviews.get(author).and_then(|o| o.as_ref())
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

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept => write!(f, "accept"),
            Self::Reject => write!(f, "reject"),
        }
    }
}

/// Code location, used for attaching comments to diffs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeLocation {
    /// Path of file.
    pub path: PathBuf,
    /// Line range on old file. `None` for added files.
    pub old: Option<Range<usize>>,
    /// Line range on new file. `None` for deleted files.
    pub new: Option<Range<usize>>,
}

impl PartialOrd for CodeLocation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CodeLocation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.path, &self.old.as_ref().map(|o| (o.start, o.end)))
            .cmp(&(&other.path, &other.new.as_ref().map(|o| (o.start, o.end))))
    }
}

/// Comment on code diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeComment {
    /// Comment author.
    author: ActorId,
    /// Code location of the comment.
    location: CodeLocation,
    /// Comment edits.
    edits: Vec<thread::Edit>,
}

impl Serialize for CodeComment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("CodeComment", 3)?;
        state.serialize_field("author", &self.author())?;
        state.serialize_field("location", self.location())?;
        state.serialize_field("body", self.body())?;
        state.end()
    }
}

impl CodeComment {
    pub fn new(
        author: ActorId,
        body: String,
        location: CodeLocation,
        timestamp: Timestamp,
    ) -> Self {
        let edit = thread::Edit { body, timestamp };

        Self {
            author,
            location,
            edits: vec![edit],
        }
    }

    /// Add an edit.
    pub fn edit(&mut self, body: String, timestamp: Timestamp) {
        self.edits.push(thread::Edit { body, timestamp });
    }

    /// Comment author.
    pub fn author(&self) -> ActorId {
        self.author
    }

    /// Get the comment location.
    pub fn location(&self) -> &CodeLocation {
        &self.location
    }

    /// Get the comment body. If there are multiple edits, gets the value at the latest edit.
    pub fn body(&self) -> &str {
        // SAFETY: There is always at least one edit. This is guaranteed by [`CodeComment::new`]
        // constructor.
        #[allow(clippy::unwrap_used)]
        self.edits.last().unwrap().body.as_str()
    }
}

/// A patch review on a revision.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Review {
    /// Review verdict.
    ///
    /// The verdict cannot be changed, since revisions are immutable.
    verdict: Option<Verdict>,
    /// Review summary.
    ///
    /// Can be edited or set to `None`.
    summary: Option<String>,
    /// Review inline code comments.
    comments: BTreeMap<EntryId, Option<CodeComment>>,
    /// Review timestamp.
    timestamp: Timestamp,
}

impl Serialize for Review {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Review", 4)?;
        state.serialize_field("verdict", &self.verdict())?;
        state.serialize_field("summary", &self.summary())?;
        state.serialize_field(
            "comments",
            &self.comments().map(|(_, c)| c.body()).collect::<Vec<_>>(),
        )?;
        state.serialize_field("timestamp", &self.timestamp())?;
        state.end()
    }
}

impl Review {
    pub fn new(verdict: Option<Verdict>, summary: Option<String>, timestamp: Timestamp) -> Self {
        Self {
            verdict,
            summary,
            comments: BTreeMap::default(),
            timestamp,
        }
    }

    /// Review verdict.
    pub fn verdict(&self) -> Option<Verdict> {
        self.verdict
    }

    /// Review inline code comments.
    pub fn comments(&self) -> impl Iterator<Item = (&EntryId, &CodeComment)> {
        self.comments
            .iter()
            .filter_map(|(id, r)| r.as_ref().map(|comment| (id, comment)))
    }

    /// Review general comment.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Review timestamp.
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
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

    pub fn edit_review(
        &mut self,
        review: EntryId,
        summary: Option<String>,
    ) -> Result<(), store::Error> {
        self.push(Action::EditReview { review, summary })
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

    /// Comment on code.
    pub fn code_comment<S: ToString>(
        &mut self,
        review: EntryId,
        body: S,
        location: CodeLocation,
    ) -> Result<(), store::Error> {
        self.push(Action::CodeComment {
            review,
            body: body.to_string(),
            location,
        })
    }

    /// Edit comment on code.
    pub fn edit_code_comment<S: ToString>(
        &mut self,
        review: EntryId,
        comment: EntryId,
        body: S,
    ) -> Result<(), store::Error> {
        self.push(Action::EditCodeComment {
            review,
            comment,
            body: body.to_string(),
        })
    }

    /// Review a patch revision.
    pub fn review(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        summary: Option<String>,
    ) -> Result<(), store::Error> {
        self.push(Action::Review {
            revision,
            summary,
            verdict,
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

pub struct PatchMut<'a, 'g, R> {
    pub id: ObjectId,

    patch: Patch,
    store: &'g mut Patches<'a, R>,
}

impl<'a, 'g, R> PatchMut<'a, 'g, R>
where
    R: ReadRepository + SignRepository + cob::Store,
{
    pub fn new(id: ObjectId, patch: Patch, store: &'g mut Patches<'a, R>) -> Self {
        Self { id, patch, store }
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
        let mut tx = Transaction::new(*signer.public_key());
        operations(&mut tx)?;
        let (op, commit) = tx.commit(message, self.id, &mut self.store.raw, signer)?;

        self.patch.apply(op, self.store.as_ref())?;

        Ok(commit)
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

    /// Edit review.
    pub fn edit_review<G: Signer>(
        &mut self,
        review: EntryId,
        summary: Option<String>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit review", signer, |tx| tx.edit_review(review, summary))
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

    /// Comment on a line of code as part of a review.
    pub fn code_comment<G: Signer, S: ToString>(
        &mut self,
        review: EntryId,
        body: S,
        location: CodeLocation,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Code comment", signer, |tx| {
            tx.code_comment(review, body, location)
        })
    }

    /// Edit comment on code.
    pub fn edit_code_comment<G: Signer, S: ToString>(
        &mut self,
        review: EntryId,
        comment: EntryId,
        body: S,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit code comment", signer, |tx| {
            tx.edit_code_comment(review, comment, body)
        })
    }

    /// Review a patch revision.
    pub fn review<G: Signer>(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        comment: Option<String>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Review", signer, |tx| tx.review(revision, verdict, comment))
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

impl<'a, 'g, R> Deref for PatchMut<'a, 'g, R> {
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

pub struct Patches<'a, R> {
    raw: store::Store<'a, Patch, R>,
}

impl<'a, R> Deref for Patches<'a, R> {
    type Target = store::Store<'a, Patch, R>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a, R> Patches<'a, R>
where
    R: ReadRepository + cob::Store,
{
    /// Open an patches store.
    pub fn open(repository: &'a R) -> Result<Self, store::Error> {
        let raw = store::Store::open(repository)?;

        Ok(Self { raw })
    }

    /// Patches count by state.
    pub fn counts(&self) -> Result<PatchCounts, store::Error> {
        let all = self.all()?;
        let state_groups =
            all.filter_map(|s| s.ok())
                .fold(PatchCounts::default(), |mut state, (_, p)| {
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
            .find_map(|(p_id, p)| p.revision(id).map(|r| (p_id, p.clone(), r.clone())));
        Ok(result)
    }

    /// Get a patch.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Patch>, store::Error> {
        self.raw.get(id)
    }

    /// Get proposed patches.
    pub fn proposed(&self) -> Result<impl Iterator<Item = (PatchId, Patch)> + '_, Error> {
        let all = self.all()?;

        Ok(all
            .into_iter()
            .filter_map(|result| result.ok())
            .filter(|(_, p)| p.is_open()))
    }

    /// Get patches proposed by the given key.
    pub fn proposed_by<'b>(
        &'b self,
        who: &'b Did,
    ) -> Result<impl Iterator<Item = (PatchId, Patch)> + '_, Error> {
        Ok(self
            .proposed()?
            .filter(move |(_, p)| p.author().id() == who))
    }
}

impl<'a, R> Patches<'a, R>
where
    R: ReadRepository + SignRepository + cob::Store,
{
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
    ) -> Result<PatchMut<'a, 'g, R>, Error> {
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
    ) -> Result<PatchMut<'a, 'g, R>, Error> {
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

    /// Get a patch mutably.
    pub fn get_mut<'g>(&'g mut self, id: &ObjectId) -> Result<PatchMut<'a, 'g, R>, store::Error> {
        let patch = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(PatchMut {
            id: *id,
            patch,
            store: self,
        })
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
    ) -> Result<PatchMut<'a, 'g, R>, Error> {
        let (id, patch) = Transaction::initial("Create patch", &mut self.raw, signer, |tx| {
            tx.revision(description, base, oid)?;
            tx.edit(title, target)?;
            tx.tag(tags.to_owned(), [])?;

            if state != State::default() {
                tx.lifecycle(state)?;
            }
            Ok(())
        })?;

        Ok(PatchMut::new(id, patch, self))
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cob::test::Actor;
    use crate::crypto::test::signer::MockSigner;
    use crate::test;
    use crate::test::arbitrary;
    use crate::test::arbitrary::gen;
    use crate::test::storage::MockRepository;

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
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let author: Did = alice.signer.public_key().into();
        let target = MergeTarget::Delegates;
        let patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                target,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

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
        assert_eq!(revision.oid, branch.oid);
        assert_eq!(revision.base, branch.base);

        let (id, _, _) = patches.find_by_revision(rev_id).unwrap().unwrap();
        assert_eq!(id, patch_id);
    }

    #[test]
    fn test_patch_discussion() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (revision_id, _) = patch.revisions().last().unwrap();
        assert!(
            patch
                .comment(*revision_id, "patch comment", None, &alice.signer)
                .is_ok(),
            "can comment on patch"
        );

        let (_, revision) = patch.revisions().last().unwrap();
        let (_, comment) = revision.discussion.first().unwrap();
        assert_eq!("patch comment", comment.body(), "comment body untouched");
    }

    #[test]
    fn test_patch_merge() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        let id = patch.id;
        let (rid, _) = patch.revisions().next().unwrap();
        let _merge = patch.merge(*rid, branch.base, &alice.signer).unwrap();

        let patch = patches.get(&id).unwrap().unwrap();

        let merges = patch.merges.iter().collect::<Vec<_>>();
        assert_eq!(merges.len(), 1);

        let (merger, merge) = merges.first().unwrap();
        assert_eq!(*merger, alice.signer.public_key());
        assert_eq!(merge.commit, branch.base);
    }

    #[test]
    fn test_patch_review() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        patch
            .review(
                *rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                &alice.signer,
            )
            .unwrap();

        let id = patch.id;
        let patch = patches.get(&id).unwrap().unwrap();
        let (_, revision) = patch.latest();
        assert_eq!(revision.reviews.len(), 1);

        let review = revision.review(alice.signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.summary(), Some("LGTM"));
    }

    #[test]
    fn test_revision_review_merge_redacted() {
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
            summary: None,
            verdict: Some(Verdict::Accept),
        });
        let a4 = alice.op(Action::Merge {
            revision: a1.id(),
            commit: oid,
        });

        patch.apply(a1, &repo).unwrap();
        assert!(patch.revisions().next().is_some());

        patch.apply(a2, &repo).unwrap();
        assert!(patch.revisions().next().is_none());

        patch.apply(a3, &repo).unwrap();
        patch.apply(a4, &repo).unwrap();
    }

    #[test]
    fn test_revision_edit_redact() {
        let base = arbitrary::oid();
        let oid = arbitrary::oid();
        let repo = gen::<MockRepository>(1);
        let time = Timestamp::now();
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let mut h0: cob::test::HistoryBuilder<Patch> = cob::test::history(
            &Action::Revision {
                description: String::from("Original"),
                base,
                oid,
            },
            time,
            &alice,
        );
        h0.commit(
            &Action::Edit {
                title: String::from("Some patch"),
                target: MergeTarget::Delegates,
            },
            &alice,
        );

        let mut h1 = h0.clone();
        h1.commit(
            &Action::Redact {
                revision: h0.root(),
            },
            &alice,
        );

        let mut h2 = h0.clone();
        h2.commit(
            &Action::EditRevision {
                revision: h0.root(),
                description: String::from("Edited"),
            },
            &bob,
        );

        h0.merge(h1);
        h0.merge(h2);

        let patch = Patch::from_history(&h0, &repo).unwrap();
        assert_eq!(patch.revisions().count(), 0);
    }

    #[test]
    fn test_patch_review_edit() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        let rid = *rid;

        let review = patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                &alice.signer,
            )
            .unwrap();
        patch
            .edit_review(review, Some("Whoops!".to_owned()), &alice.signer)
            .unwrap(); // Overwrite the comment.
                       //
        let (_, revision) = patch.latest();
        let review = revision.review(alice.signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.summary(), Some("Whoops!"));
    }

    #[test]
    fn test_patch_review_comment() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        let rid = *rid;
        let location = CodeLocation {
            path: PathBuf::from_str("README").unwrap(),
            old: None,
            new: Some(5..8),
        };
        let review = patch.review(rid, None, None, &alice.signer).unwrap();
        patch
            .code_comment(
                review,
                "I like these lines of code",
                location.clone(),
                &alice.signer,
            )
            .unwrap();

        let (_, revision) = patch.latest();
        let review = revision.review(alice.signer.public_key()).unwrap();
        let (_, comment) = review.comments().next().unwrap();

        assert_eq!(comment.body(), "I like these lines of code");
        assert_eq!(comment.location(), &location);
    }

    #[test]
    fn test_patch_review_remove_summary() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        let (rid, _) = patch.latest();
        let rid = *rid;
        let review = patch
            .review(rid, None, Some("Nah".to_owned()), &alice.signer)
            .unwrap();
        patch.edit_review(review, None, &alice.signer).unwrap();

        let id = patch.id;
        let patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest();
        let review = revision.review(alice.signer.public_key()).unwrap();

        assert_eq!(review.summary(), None);
    }

    #[test]
    fn test_patch_update() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Patches::open(&*alice.repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();

        assert_eq!(patch.description(), "Blah blah blah.");
        assert_eq!(patch.version(), 0);

        let update = checkout.branch_with([("README", b"Hello Radicle!")]);
        let _ = patch
            .update("I've made changes.", branch.base, update.oid, &alice.signer)
            .unwrap();

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
        let alice = test::setup::Node::default();
        let repo = alice.project();
        let branch = repo
            .checkout()
            .branch_with([("README.md", b"Hello, World!")]);
        let mut patches = Patches::open(&*repo).unwrap();
        let mut patch = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                MergeTarget::Delegates,
                branch.base,
                branch.oid,
                &[],
                &alice.signer,
            )
            .unwrap();
        let patch_id = patch.id;

        let update = repo
            .checkout()
            .branch_with([("README.md", b"Hello, Radicle!")]);
        let revision_id = patch
            .update("I've made changes.", branch.base, update.oid, &alice.signer)
            .unwrap();
        assert_eq!(patch.revisions().count(), 2);

        patch.redact(revision_id, &alice.signer).unwrap();
        assert_eq!(patch.latest().0, &RevisionId::from(patch_id));
        assert_eq!(patch.revisions().count(), 1);

        // The patch's root must always exist.
        assert!(patch.redact(*patch.latest().0, &alice.signer).is_err());
    }
}
