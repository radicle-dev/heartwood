pub mod cache;

use std::collections::btree_map;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use amplify::Wrapper;
use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use storage::{HasRepoId, RepositoryError};
use thiserror::Error;

use crate::cob;
use crate::cob::common::{Author, Authorization, CodeLocation, Label, Reaction, Timestamp};
use crate::cob::store::Transaction;
use crate::cob::store::{Cob, CobAction};
use crate::cob::thread;
use crate::cob::thread::Thread;
use crate::cob::thread::{Comment, CommentId, Edit, Reactions};
use crate::cob::{op, store, ActorId, Embed, EntryId, ObjectId, TypeName, Uri};
use crate::crypto::{PublicKey, Signer};
use crate::git;
use crate::identity::doc::{DocAt, DocError};
use crate::identity::PayloadError;
use crate::prelude::*;
use crate::storage;

pub use cache::Cache;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.patch").expect("type name is valid"));

/// Patch operation.
pub type Op = cob::Op<Action>;

/// Identifier for a patch.
pub type PatchId = ObjectId;

/// Unique identifier for a patch revision.
#[derive(
    Wrapper,
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    From,
    Display,
)]
#[display(inner)]
#[wrap(Deref)]
pub struct RevisionId(EntryId);

/// Unique identifier for a patch review.
#[derive(
    Wrapper,
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    From,
    Display,
)]
#[display(inner)]
#[wrapper(Deref)]
pub struct ReviewId(EntryId);

/// Index of a revision in the revisions list.
pub type RevisionIx = usize;

/// Error applying an operation onto a state.
#[derive(Debug, Error)]
pub enum Error {
    /// Causal dependency missing.
    ///
    /// This error indicates that the operations are not being applied
    /// in causal order, which is a requirement for this CRDT.
    ///
    /// For example, this can occur if an operation references another operation
    /// that hasn't happened yet.
    #[error("causal dependency {0:?} missing")]
    Missing(EntryId),
    /// Error applying an op to the patch thread.
    #[error("thread apply failed: {0}")]
    Thread(#[from] thread::Error),
    /// Error loading the identity document committed to by an operation.
    #[error("identity doc failed to load: {0}")]
    Doc(#[from] DocError),
    /// Identity document is missing.
    #[error("missing identity document")]
    MissingIdentity,
    /// Review is empty.
    #[error("empty review; verdict or summary not provided")]
    EmptyReview,
    /// Duplicate review.
    #[error("review {0} of {1} already exists by author {2}")]
    DuplicateReview(ReviewId, RevisionId, NodeId),
    /// Error loading the document payload.
    #[error("payload failed to load: {0}")]
    Payload(#[from] PayloadError),
    /// Git error.
    #[error("git: {0}")]
    Git(#[from] git::ext::Error),
    /// Store error.
    #[error("store: {0}")]
    Store(#[from] store::Error),
    #[error("op decoding failed: {0}")]
    Op(#[from] op::OpEncodingError),
    /// Action not authorized by the author
    #[error("{0} not authorized to apply {1:?}")]
    NotAuthorized(ActorId, Action),
    /// An illegal action.
    #[error("action is not allowed: {0}")]
    NotAllowed(EntryId),
    /// Revision not found.
    #[error("revision not found: {0}")]
    RevisionNotFound(RevisionId),
    /// Initialization failed.
    #[error("initialization failed: {0}")]
    Init(&'static str),
    #[error("failed to update patch {id} in cache: {err}")]
    CacheUpdate {
        id: PatchId,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove patch {id} from cache: {err}")]
    CacheRemove {
        id: PatchId,
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove patches from cache: {err}")]
    CacheRemoveAll {
        #[source]
        err: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

/// Patch operation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    //
    // Actions on patch.
    //
    #[serde(rename = "edit")]
    Edit { title: String, target: MergeTarget },
    #[serde(rename = "label")]
    Label { labels: BTreeSet<Label> },
    #[serde(rename = "lifecycle")]
    Lifecycle { state: Lifecycle },
    #[serde(rename = "assign")]
    Assign { assignees: BTreeSet<Did> },
    #[serde(rename = "merge")]
    Merge {
        revision: RevisionId,
        commit: git::Oid,
    },

    //
    // Review actions
    //
    #[serde(rename = "review")]
    Review {
        revision: RevisionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verdict: Option<Verdict>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        labels: Vec<Label>,
    },
    #[serde(rename = "review.edit")]
    ReviewEdit {
        review: ReviewId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verdict: Option<Verdict>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        labels: Vec<Label>,
    },
    #[serde(rename = "review.redact")]
    ReviewRedact { review: ReviewId },
    #[serde(rename = "review.comment")]
    ReviewComment {
        review: ReviewId,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<CodeLocation>,
        /// Comment this is a reply to.
        /// Should be [`None`] if it's the first comment.
        /// Should be [`Some`] otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<CommentId>,
        /// Embeded content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        embeds: Vec<Embed<Uri>>,
    },
    #[serde(rename = "review.comment.edit")]
    ReviewCommentEdit {
        review: ReviewId,
        comment: EntryId,
        body: String,
        embeds: Vec<Embed<Uri>>,
    },
    #[serde(rename = "review.comment.redact")]
    ReviewCommentRedact { review: ReviewId, comment: EntryId },
    #[serde(rename = "review.comment.react")]
    ReviewCommentReact {
        review: ReviewId,
        comment: EntryId,
        reaction: Reaction,
        active: bool,
    },
    #[serde(rename = "review.comment.resolve")]
    ReviewCommentResolve { review: ReviewId, comment: EntryId },
    #[serde(rename = "review.comment.unresolve")]
    ReviewCommentUnresolve { review: ReviewId, comment: EntryId },

    //
    // Revision actions
    //
    #[serde(rename = "revision")]
    Revision {
        description: String,
        base: git::Oid,
        oid: git::Oid,
        /// Review comments resolved by this revision.
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        resolves: BTreeSet<(EntryId, CommentId)>,
    },
    #[serde(rename = "revision.edit")]
    RevisionEdit {
        revision: RevisionId,
        description: String,
        /// Embeded content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        embeds: Vec<Embed<Uri>>,
    },
    /// React to the revision.
    #[serde(rename = "revision.react")]
    RevisionReact {
        revision: RevisionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<CodeLocation>,
        reaction: Reaction,
        active: bool,
    },
    #[serde(rename = "revision.redact")]
    RevisionRedact { revision: RevisionId },
    #[serde(rename_all = "camelCase")]
    #[serde(rename = "revision.comment")]
    RevisionComment {
        /// The revision to comment on.
        revision: RevisionId,
        /// For comments on the revision code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<CodeLocation>,
        /// Comment body.
        body: String,
        /// Comment this is a reply to.
        /// Should be [`None`] if it's the top-level comment.
        /// Should be the root [`CommentId`] if it's a top-level comment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<CommentId>,
        /// Embeded content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        embeds: Vec<Embed<Uri>>,
    },
    /// Edit a revision comment.
    #[serde(rename = "revision.comment.edit")]
    RevisionCommentEdit {
        revision: RevisionId,
        comment: CommentId,
        body: String,
        embeds: Vec<Embed<Uri>>,
    },
    /// Redact a revision comment.
    #[serde(rename = "revision.comment.redact")]
    RevisionCommentRedact {
        revision: RevisionId,
        comment: CommentId,
    },
    /// React to a revision comment.
    #[serde(rename = "revision.comment.react")]
    RevisionCommentReact {
        revision: RevisionId,
        comment: CommentId,
        reaction: Reaction,
        active: bool,
    },
}

impl CobAction for Action {
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

    fn produces_identifier(&self) -> bool {
        matches!(
            self,
            Self::Revision { .. }
                | Self::RevisionComment { .. }
                | Self::Review { .. }
                | Self::ReviewComment { .. }
        )
    }
}

/// Output of a merge.
#[derive(Debug)]
#[must_use]
pub struct Merged<'a, R> {
    pub patch: PatchId,
    pub entry: EntryId,

    stored: &'a R,
}

impl<R: WriteRepository> Merged<'_, R> {
    /// Cleanup after merging a patch.
    ///
    /// This removes Git refs relating to the patch, both in the working copy,
    /// and the stored copy; and updates `rad/sigrefs`.
    pub fn cleanup<G: Signer>(
        self,
        working: &git::raw::Repository,
        signer: &G,
    ) -> Result<(), storage::RepositoryError> {
        let nid = signer.public_key();
        let stored_ref = git::refs::patch(&self.patch).with_namespace(nid.into());
        let working_ref = git::refs::workdir::patch_upstream(&self.patch);

        working
            .find_reference(&working_ref)
            .and_then(|mut r| r.delete())
            .ok();

        self.stored
            .raw()
            .find_reference(&stored_ref)
            .and_then(|mut r| r.delete())
            .ok();
        self.stored.sign_refs(signer)?;

        Ok(())
    }
}

/// Where a patch is intended to be merged.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub fn head<R: ReadRepository>(&self, repo: &R) -> Result<git::Oid, RepositoryError> {
        match self {
            MergeTarget::Delegates => {
                let (_, target) = repo.head()?;
                Ok(target)
            }
        }
    }
}

/// Patch state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    /// Title of the patch.
    pub(super) title: String,
    /// Patch author.
    pub(super) author: Author,
    /// Current state of the patch.
    pub(super) state: State,
    /// Target this patch is meant to be merged in.
    pub(super) target: MergeTarget,
    /// Associated labels.
    /// Labels can be added and removed at will.
    pub(super) labels: BTreeSet<Label>,
    /// Patch merges.
    ///
    /// Only one merge is allowed per user.
    ///
    /// Merges can be removed and replaced, but not modified. Generally, once a revision is merged,
    /// it stays that way. Being able to remove merges may be useful in case of force updates
    /// on the target branch.
    pub(super) merges: BTreeMap<ActorId, Merge>,
    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    ///
    /// Revisions can be redacted, but are otherwise immutable.
    pub(super) revisions: BTreeMap<RevisionId, Option<Revision>>,
    /// Users assigned to review this patch.
    pub(super) assignees: BTreeSet<ActorId>,
    /// Timeline of operations.
    pub(super) timeline: Vec<EntryId>,
    /// Reviews index. Keeps track of reviews for better performance.
    pub(super) reviews: BTreeMap<ReviewId, Option<(RevisionId, ActorId)>>,
}

impl Patch {
    /// Construct a new patch object from a revision.
    pub fn new(title: String, target: MergeTarget, (id, revision): (RevisionId, Revision)) -> Self {
        Self {
            title,
            author: revision.author.clone(),
            state: State::default(),
            target,
            labels: BTreeSet::default(),
            merges: BTreeMap::default(),
            revisions: BTreeMap::from_iter([(id, Some(revision))]),
            assignees: BTreeSet::default(),
            timeline: vec![id.into_inner()],
            reviews: BTreeMap::default(),
        }
    }

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
        self.updates()
            .next()
            .map(|(_, r)| r)
            .expect("Patch::timestamp: at least one revision is present")
            .timestamp
    }

    /// Associated labels.
    pub fn labels(&self) -> impl Iterator<Item = &Label> {
        self.labels.iter()
    }

    /// Patch description.
    pub fn description(&self) -> &str {
        let (_, r) = self.root();
        r.description()
    }

    /// Patch embeds.
    pub fn embeds(&self) -> &[Embed<Uri>] {
        let (_, r) = self.root();
        r.embeds()
    }

    /// Author of the first revision of the patch.
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// All revision authors.
    pub fn authors(&self) -> BTreeSet<&Author> {
        self.revisions
            .values()
            .filter_map(|r| r.as_ref())
            .map(|r| &r.author)
            .collect()
    }

    /// Get the `Revision` by its `RevisionId`.
    ///
    /// None is returned if the `Revision` has been redacted (deleted).
    pub fn revision(&self, id: &RevisionId) -> Option<&Revision> {
        self.revisions.get(id).and_then(|o| o.as_ref())
    }

    /// List of patch revisions by the patch author. The initial changeset is part of the
    /// first revision.
    pub fn updates(&self) -> impl DoubleEndedIterator<Item = (RevisionId, &Revision)> {
        self.revisions_by(self.author().public_key())
    }

    /// List of all patch revisions by all authors.
    pub fn revisions(&self) -> impl DoubleEndedIterator<Item = (RevisionId, &Revision)> {
        self.timeline.iter().filter_map(move |id| {
            self.revisions
                .get(id)
                .and_then(|o| o.as_ref())
                .map(|rev| (RevisionId(*id), rev))
        })
    }

    /// List of patch revisions by the given author.
    pub fn revisions_by<'a>(
        &'a self,
        author: &'a PublicKey,
    ) -> impl DoubleEndedIterator<Item = (RevisionId, &'a Revision)> {
        self.revisions()
            .filter(move |(_, r)| (r.author.public_key() == author))
    }

    /// List of patch reviews of the given revision.
    pub fn reviews_of(&self, rev: RevisionId) -> impl Iterator<Item = (&ReviewId, &Review)> {
        self.reviews.iter().filter_map(move |(review_id, t)| {
            t.and_then(|(rev_id, pk)| {
                if rev == rev_id {
                    self.revision(&rev_id)
                        .and_then(|r| r.review_by(&pk))
                        .map(|r| (review_id, r))
                } else {
                    None
                }
            })
        })
    }

    /// List of patch assignees.
    pub fn assignees(&self) -> impl Iterator<Item = Did> + '_ {
        self.assignees.iter().map(Did::from)
    }

    /// Get the merges.
    pub fn merges(&self) -> impl Iterator<Item = (&ActorId, &Merge)> {
        self.merges.iter()
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
    pub fn range(&self) -> Result<(git::Oid, git::Oid), git::ext::Error> {
        Ok((*self.base(), *self.head()))
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
    pub fn root(&self) -> (RevisionId, &Revision) {
        self.updates()
            .next()
            .expect("Patch::root: there is always a root revision")
    }

    /// Latest revision by the patch author.
    pub fn latest(&self) -> (RevisionId, &Revision) {
        self.latest_by(self.author().public_key())
            .expect("Patch::latest: there is always at least one revision")
    }

    /// Latest revision by the given author.
    pub fn latest_by<'a>(&'a self, author: &'a PublicKey) -> Option<(RevisionId, &'a Revision)> {
        self.revisions_by(author).next_back()
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

    /// Apply authorization rules on patch actions.
    pub fn authorization(
        &self,
        action: &Action,
        actor: &ActorId,
        doc: &Doc,
    ) -> Result<Authorization, Error> {
        if doc.is_delegate(&actor.into()) {
            // A delegate is authorized to do all actions.
            return Ok(Authorization::Allow);
        }
        let author = self.author().id().as_key();
        let outcome = match action {
            // The patch author can edit the patch and change its state.
            Action::Edit { .. } => Authorization::from(actor == author),
            Action::Lifecycle { state } => Authorization::from(match state {
                Lifecycle::Open { .. } => actor == author,
                Lifecycle::Draft { .. } => actor == author,
                Lifecycle::Archived { .. } => actor == author,
            }),
            // Only delegates can carry out these actions.
            Action::Label { labels } => {
                if labels == &self.labels {
                    // No-op is allowed for backwards compatibility.
                    Authorization::Allow
                } else {
                    Authorization::Deny
                }
            }
            Action::Assign { .. } => Authorization::Deny,
            Action::Merge { .. } => match self.target() {
                MergeTarget::Delegates => Authorization::Deny,
            },
            // Anyone can submit a review.
            Action::Review { .. } => Authorization::Allow,
            Action::ReviewRedact { review, .. } | Action::ReviewEdit { review, .. } => {
                if let Some((_, review)) = lookup::review(self, review)? {
                    Authorization::from(actor == review.author.public_key())
                } else {
                    // Redacted.
                    Authorization::Unknown
                }
            }
            // Anyone can comment on a review.
            Action::ReviewComment { .. } => Authorization::Allow,
            // The comment author can edit and redact their own comment.
            Action::ReviewCommentEdit {
                review, comment, ..
            }
            | Action::ReviewCommentRedact { review, comment } => {
                if let Some((_, review)) = lookup::review(self, review)? {
                    if let Some(comment) = review.comments.comment(comment) {
                        return Ok(Authorization::from(*actor == comment.author()));
                    }
                }
                // Redacted.
                Authorization::Unknown
            }
            // Anyone can react to a review comment.
            Action::ReviewCommentReact { .. } => Authorization::Allow,
            // The reviewer, commenter or revision author can resolve and unresolve review comments.
            Action::ReviewCommentResolve { review, comment }
            | Action::ReviewCommentUnresolve { review, comment } => {
                if let Some((revision, review)) = lookup::review(self, review)? {
                    if let Some(comment) = review.comments.comment(comment) {
                        return Ok(Authorization::from(
                            actor == &comment.author()
                                || actor == review.author.public_key()
                                || actor == revision.author.public_key(),
                        ));
                    }
                }
                // Redacted.
                Authorization::Unknown
            }
            // Anyone can propose revisions.
            Action::Revision { .. } => Authorization::Allow,
            // Only the revision author can edit or redact their revision.
            Action::RevisionEdit { revision, .. } | Action::RevisionRedact { revision, .. } => {
                if let Some(revision) = lookup::revision(self, revision)? {
                    Authorization::from(actor == revision.author.public_key())
                } else {
                    // Redacted.
                    Authorization::Unknown
                }
            }
            // Anyone can react to or comment on a revision.
            Action::RevisionReact { .. } => Authorization::Allow,
            Action::RevisionComment { .. } => Authorization::Allow,
            // Only the comment author can edit or redact their comment.
            Action::RevisionCommentEdit {
                revision, comment, ..
            }
            | Action::RevisionCommentRedact {
                revision, comment, ..
            } => {
                if let Some(revision) = lookup::revision(self, revision)? {
                    if let Some(comment) = revision.discussion.comment(comment) {
                        return Ok(Authorization::from(actor == &comment.author()));
                    }
                }
                // Redacted.
                Authorization::Unknown
            }
            // Anyone can react to a revision.
            Action::RevisionCommentReact { .. } => Authorization::Allow,
        };
        Ok(outcome)
    }
}

impl Patch {
    /// Apply an action after checking if it's authorized.
    fn op_action<R: ReadRepository>(
        &mut self,
        action: Action,
        id: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        concurrent: &[&cob::Entry],
        doc: &DocAt,
        repo: &R,
    ) -> Result<(), Error> {
        match self.authorization(&action, &author, doc)? {
            Authorization::Allow => {
                self.action(action, id, author, timestamp, concurrent, doc, repo)
            }
            Authorization::Deny => Err(Error::NotAuthorized(author, action)),
            Authorization::Unknown => {
                // In this case, since there is not enough information to determine
                // whether the action is authorized or not, we simply ignore it.
                // It's likely that the target object was redacted, and we can't
                // verify whether the action would have been allowed or not.
                Ok(())
            }
        }
    }

    /// Apply a single action to the patch.
    fn action<R: ReadRepository>(
        &mut self,
        action: Action,
        entry: EntryId,
        author: ActorId,
        timestamp: Timestamp,
        _concurrent: &[&cob::Entry],
        identity: &Doc,
        repo: &R,
    ) -> Result<(), Error> {
        match action {
            Action::Edit { title, target } => {
                self.title = title;
                self.target = target;
            }
            Action::Lifecycle { state } => {
                let valid = self.state == State::Draft
                    || self.state == State::Archived
                    || self.state == State::Open { conflicts: vec![] };

                if valid {
                    match state {
                        Lifecycle::Open => {
                            self.state = State::Open { conflicts: vec![] };
                        }
                        Lifecycle::Draft => {
                            self.state = State::Draft;
                        }
                        Lifecycle::Archived => {
                            self.state = State::Archived;
                        }
                    }
                }
            }
            Action::Label { labels } => {
                self.labels = BTreeSet::from_iter(labels);
            }
            Action::Assign { assignees } => {
                self.assignees = BTreeSet::from_iter(assignees.into_iter().map(ActorId::from));
            }
            Action::RevisionEdit {
                revision,
                description,
                embeds,
            } => {
                if let Some(redactable) = self.revisions.get_mut(&revision) {
                    // If the revision was redacted concurrently, there's nothing to do.
                    if let Some(revision) = redactable {
                        revision.description.push(Edit::new(
                            author,
                            description,
                            timestamp,
                            embeds,
                        ));
                    }
                } else {
                    return Err(Error::Missing(revision.into_inner()));
                }
            }
            Action::Revision {
                description,
                base,
                oid,
                resolves,
            } => {
                debug_assert!(!self.revisions.contains_key(&entry));
                let id = RevisionId(entry);

                self.revisions.insert(
                    id,
                    Some(Revision::new(
                        id,
                        author.into(),
                        description,
                        base,
                        oid,
                        timestamp,
                        resolves,
                    )),
                );
            }
            Action::RevisionReact {
                revision,
                reaction,
                active,
                location,
            } => {
                if let Some(revision) = lookup::revision_mut(self, &revision)? {
                    let key = (author, reaction);
                    let reactions = revision.reactions.entry(location).or_default();

                    if active {
                        reactions.insert(key);
                    } else {
                        reactions.remove(&key);
                    }
                }
            }
            Action::RevisionRedact { revision } => {
                // Not allowed to delete the root revision.
                let (root, _) = self.root();
                if revision == root {
                    return Err(Error::NotAllowed(entry));
                }
                // Redactions must have observed a revision to be valid.
                if let Some(r) = self.revisions.get_mut(&revision) {
                    // If the revision has already been merged, ignore the redaction. We
                    // don't want to redact merged revisions.
                    if self.merges.values().any(|m| m.revision == revision) {
                        return Ok(());
                    }
                    *r = None;
                } else {
                    return Err(Error::Missing(revision.into_inner()));
                }
            }
            Action::Review {
                revision,
                ref summary,
                verdict,
                labels,
            } => {
                let Some(rev) = self.revisions.get_mut(&revision) else {
                    // If the revision was redacted concurrently, there's nothing to do.
                    return Ok(());
                };
                if let Some(rev) = rev {
                    // Insert a review if there isn't already one. Otherwise we just ignore
                    // this operation
                    if let btree_map::Entry::Vacant(e) = rev.reviews.entry(author) {
                        let id = ReviewId(entry);

                        e.insert(Review::new(
                            id,
                            Author::new(author),
                            verdict,
                            summary.to_owned(),
                            labels,
                            timestamp,
                        ));
                        // Update reviews index.
                        self.reviews.insert(id, Some((revision, author)));
                    } else {
                        log::error!(
                            target: "patch",
                            "Review by {author} for {revision} already exists, ignoring action.."
                        );
                    }
                }
            }
            Action::ReviewEdit {
                review,
                summary,
                verdict,
                labels,
            } => {
                if summary.is_none() && verdict.is_none() {
                    return Err(Error::EmptyReview);
                }
                let Some(review) = lookup::review_mut(self, &review)? else {
                    return Ok(());
                };
                review.verdict = verdict;
                review.summary = summary;
                review.labels = labels;
            }
            Action::ReviewCommentReact {
                review,
                comment,
                reaction,
                active,
            } => {
                if let Some(review) = lookup::review_mut(self, &review)? {
                    thread::react(
                        &mut review.comments,
                        entry,
                        author,
                        comment,
                        reaction,
                        active,
                    )?;
                }
            }
            Action::ReviewCommentRedact { review, comment } => {
                if let Some(review) = lookup::review_mut(self, &review)? {
                    thread::redact(&mut review.comments, entry, comment)?;
                }
            }
            Action::ReviewCommentEdit {
                review,
                comment,
                body,
                embeds,
            } => {
                if let Some(review) = lookup::review_mut(self, &review)? {
                    thread::edit(
                        &mut review.comments,
                        entry,
                        author,
                        comment,
                        timestamp,
                        body,
                        embeds,
                    )?;
                }
            }
            Action::ReviewCommentResolve { review, comment } => {
                if let Some(review) = lookup::review_mut(self, &review)? {
                    thread::resolve(&mut review.comments, entry, comment)?;
                }
            }
            Action::ReviewCommentUnresolve { review, comment } => {
                if let Some(review) = lookup::review_mut(self, &review)? {
                    thread::unresolve(&mut review.comments, entry, comment)?;
                }
            }
            Action::ReviewComment {
                review,
                body,
                location,
                reply_to,
                embeds,
            } => {
                if let Some(review) = lookup::review_mut(self, &review)? {
                    thread::comment(
                        &mut review.comments,
                        entry,
                        author,
                        timestamp,
                        body,
                        reply_to,
                        location,
                        embeds,
                    )?;
                }
            }
            Action::ReviewRedact { review } => {
                // Redactions must have observed a review to be valid.
                let Some(locator) = self.reviews.get_mut(&review) else {
                    return Err(Error::Missing(review.into_inner()));
                };
                // If the review is already redacted, do nothing.
                let Some((revision, reviewer)) = locator else {
                    return Ok(());
                };
                // The revision must have existed at some point.
                let Some(redactable) = self.revisions.get_mut(revision) else {
                    return Err(Error::Missing(revision.into_inner()));
                };
                // But it could be redacted.
                let Some(revision) = redactable else {
                    return Ok(());
                };
                // Remove review for this author.
                if let Some(r) = revision.reviews.remove(reviewer) {
                    debug_assert_eq!(r.id, review);
                } else {
                    log::error!(
                        target: "patch", "Review {review} not found in revision {}", revision.id
                    );
                }
                // Set the review locator in the review index to redacted.
                *locator = None;
            }
            Action::Merge { revision, commit } => {
                // If the revision was redacted before the merge, ignore the merge.
                if lookup::revision_mut(self, &revision)?.is_none() {
                    return Ok(());
                };
                match self.target() {
                    MergeTarget::Delegates => {
                        let proj = identity.project()?;
                        let branch = git::refs::branch(proj.default_branch());

                        // Nb. We don't return an error in case the merge commit is not an
                        // ancestor of the default branch. The default branch can change
                        // *after* the merge action is created, which is out of the control
                        // of the merge author. We simply skip it, which allows archiving in
                        // case of a rebase off the master branch, or a redaction of the
                        // merge.
                        let Ok(head) = repo.reference_oid(&author, &branch) else {
                            return Ok(());
                        };
                        if commit != head && !repo.is_ancestor_of(commit, head)? {
                            return Ok(());
                        }
                    }
                }
                self.merges.insert(
                    author,
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

                {
                    if let Some(threshold) = identity.default_branch_threshold()? {
                        // Discard revisions that weren't merged by a threshold of delegates.
                        merges.retain(|_, count| *count >= threshold);
                    }
                }

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

            Action::RevisionComment {
                revision,
                body,
                reply_to,
                embeds,
                location,
            } => {
                if let Some(revision) = lookup::revision_mut(self, &revision)? {
                    thread::comment(
                        &mut revision.discussion,
                        entry,
                        author,
                        timestamp,
                        body,
                        reply_to,
                        location,
                        embeds,
                    )?;
                }
            }
            Action::RevisionCommentEdit {
                revision,
                comment,
                body,
                embeds,
            } => {
                if let Some(revision) = lookup::revision_mut(self, &revision)? {
                    thread::edit(
                        &mut revision.discussion,
                        entry,
                        author,
                        comment,
                        timestamp,
                        body,
                        embeds,
                    )?;
                }
            }
            Action::RevisionCommentRedact { revision, comment } => {
                if let Some(revision) = lookup::revision_mut(self, &revision)? {
                    thread::redact(&mut revision.discussion, entry, comment)?;
                }
            }
            Action::RevisionCommentReact {
                revision,
                comment,
                reaction,
                active,
            } => {
                if let Some(revision) = lookup::revision_mut(self, &revision)? {
                    thread::react(
                        &mut revision.discussion,
                        entry,
                        author,
                        comment,
                        reaction,
                        active,
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl cob::store::CobWithType for Patch {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }
}

impl store::Cob for Patch {
    type Action = Action;
    type Error = Error;

    fn from_root<R: ReadRepository>(op: Op, repo: &R) -> Result<Self, Self::Error> {
        let doc = op.identity_doc(repo)?.ok_or(Error::MissingIdentity)?;
        let mut actions = op.actions.into_iter();
        let Some(Action::Revision {
            description,
            base,
            oid,
            resolves,
        }) = actions.next()
        else {
            return Err(Error::Init("the first action must be of type `revision`"));
        };
        let Some(Action::Edit { title, target }) = actions.next() else {
            return Err(Error::Init("the second action must be of type `edit`"));
        };
        let revision = Revision::new(
            RevisionId(op.id),
            op.author.into(),
            description,
            base,
            oid,
            op.timestamp,
            resolves,
        );
        let mut patch = Patch::new(title, target, (RevisionId(op.id), revision));

        for action in actions {
            match patch.authorization(&action, &op.author, &doc)? {
                Authorization::Allow => {
                    patch.action(action, op.id, op.author, op.timestamp, &[], &doc, repo)?;
                }
                Authorization::Deny => {
                    return Err(Error::NotAuthorized(op.author, action));
                }
                Authorization::Unknown => {
                    // Note that this shouldn't really happen since there's no concurrency in the
                    // root operation.
                    continue;
                }
            }
        }
        Ok(patch)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a cob::Entry>>(
        &mut self,
        op: Op,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Error> {
        debug_assert!(!self.timeline.contains(&op.id));
        self.timeline.push(op.id);

        let doc = op.identity_doc(repo)?.ok_or(Error::MissingIdentity)?;
        let concurrent = concurrent.into_iter().collect::<Vec<_>>();

        for action in op.actions {
            log::trace!(target: "patch", "Applying {} {action:?}", op.id);

            if let Err(e) = self.op_action(
                action,
                op.id,
                op.author,
                op.timestamp,
                &concurrent,
                &doc,
                repo,
            ) {
                log::error!(target: "patch", "Error applying {}: {e}", op.id);
                return Err(e);
            }
        }
        Ok(())
    }
}

impl<R: ReadRepository> cob::Evaluate<R> for Patch {
    type Error = Error;

    fn init(entry: &cob::Entry, repo: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let object = Patch::from_root(op, repo)?;

        Ok(object)
    }

    fn apply<'a, I: Iterator<Item = (&'a EntryId, &'a cob::Entry)>>(
        &mut self,
        entry: &cob::Entry,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Self::Error> {
        let op = Op::try_from(entry)?;

        self.op(op, concurrent.map(|(_, e)| e), repo)
    }
}

mod lookup {
    use super::*;

    pub fn revision<'a>(
        patch: &'a Patch,
        revision: &RevisionId,
    ) -> Result<Option<&'a Revision>, Error> {
        match patch.revisions.get(revision) {
            Some(Some(revision)) => Ok(Some(revision)),
            // Redacted.
            Some(None) => Ok(None),
            // Missing. Causal error.
            None => Err(Error::Missing(revision.into_inner())),
        }
    }

    pub fn revision_mut<'a>(
        patch: &'a mut Patch,
        revision: &RevisionId,
    ) -> Result<Option<&'a mut Revision>, Error> {
        match patch.revisions.get_mut(revision) {
            Some(Some(revision)) => Ok(Some(revision)),
            // Redacted.
            Some(None) => Ok(None),
            // Missing. Causal error.
            None => Err(Error::Missing(revision.into_inner())),
        }
    }

    pub fn review<'a>(
        patch: &'a Patch,
        review: &ReviewId,
    ) -> Result<Option<(&'a Revision, &'a Review)>, Error> {
        match patch.reviews.get(review) {
            Some(Some((revision, author))) => {
                match patch.revisions.get(revision) {
                    Some(Some(rev)) => {
                        let r = rev
                            .reviews
                            .get(author)
                            .ok_or_else(|| Error::Missing(review.into_inner()))?;
                        debug_assert_eq!(&r.id, review);

                        Ok(Some((rev, r)))
                    }
                    Some(None) => {
                        // If the revision was redacted concurrently, there's nothing to do.
                        // Likewise, if the review was redacted concurrently, there's nothing to do.
                        Ok(None)
                    }
                    None => Err(Error::Missing(revision.into_inner())),
                }
            }
            Some(None) => {
                // Redacted.
                Ok(None)
            }
            None => Err(Error::Missing(review.into_inner())),
        }
    }

    pub fn review_mut<'a>(
        patch: &'a mut Patch,
        review: &ReviewId,
    ) -> Result<Option<&'a mut Review>, Error> {
        match patch.reviews.get(review) {
            Some(Some((revision, author))) => {
                match patch.revisions.get_mut(revision) {
                    Some(Some(rev)) => {
                        let r = rev
                            .reviews
                            .get_mut(author)
                            .ok_or_else(|| Error::Missing(review.into_inner()))?;
                        debug_assert_eq!(&r.id, review);

                        Ok(Some(r))
                    }
                    Some(None) => {
                        // If the revision was redacted concurrently, there's nothing to do.
                        // Likewise, if the review was redacted concurrently, there's nothing to do.
                        Ok(None)
                    }
                    None => Err(Error::Missing(revision.into_inner())),
                }
            }
            Some(None) => {
                // Redacted.
                Ok(None)
            }
            None => Err(Error::Missing(review.into_inner())),
        }
    }
}

/// A patch revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    /// Revision identifier.
    pub(super) id: RevisionId,
    /// Author of the revision.
    pub(super) author: Author,
    /// Revision description.
    pub(super) description: NonEmpty<Edit>,
    /// Base branch commit, used as a merge base.
    pub(super) base: git::Oid,
    /// Reference to the Git object containing the code (revision head).
    pub(super) oid: git::Oid,
    /// Discussion around this revision.
    pub(super) discussion: Thread<Comment<CodeLocation>>,
    /// Reviews of this revision's changes (all review edits are kept).
    pub(super) reviews: BTreeMap<ActorId, Review>,
    /// When this revision was created.
    pub(super) timestamp: Timestamp,
    /// Review comments resolved by this revision.
    pub(super) resolves: BTreeSet<(EntryId, CommentId)>,
    /// Reactions on code locations and revision itself
    #[serde(
        serialize_with = "ser::serialize_reactions",
        deserialize_with = "ser::deserialize_reactions"
    )]
    pub(super) reactions: BTreeMap<Option<CodeLocation>, Reactions>,
}

impl Revision {
    pub fn new(
        id: RevisionId,
        author: Author,
        description: String,
        base: git::Oid,
        oid: git::Oid,
        timestamp: Timestamp,
        resolves: BTreeSet<(EntryId, CommentId)>,
    ) -> Self {
        let description = Edit::new(*author.public_key(), description, timestamp, Vec::default());

        Self {
            id,
            author,
            description: NonEmpty::new(description),
            base,
            oid,
            discussion: Thread::default(),
            reviews: BTreeMap::default(),
            timestamp,
            resolves,
            reactions: Default::default(),
        }
    }

    pub fn id(&self) -> RevisionId {
        self.id
    }

    pub fn description(&self) -> &str {
        self.description.last().body.as_str()
    }

    pub fn edits(&self) -> impl Iterator<Item = &Edit> {
        self.description.iter()
    }

    pub fn embeds(&self) -> &[Embed<Uri>] {
        &self.description.last().embeds
    }

    pub fn reactions(&self) -> &BTreeMap<Option<CodeLocation>, BTreeSet<(PublicKey, Reaction)>> {
        &self.reactions
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

    /// Get the commit range of this revision.
    pub fn range(&self) -> (git::Oid, git::Oid) {
        (self.base, self.oid)
    }

    /// When this revision was created.
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Discussion around this revision.
    pub fn discussion(&self) -> &Thread<Comment<CodeLocation>> {
        &self.discussion
    }

    /// Review comments resolved by this revision.
    pub fn resolves(&self) -> &BTreeSet<(EntryId, CommentId)> {
        &self.resolves
    }

    /// Iterate over all top-level replies.
    pub fn replies(&self) -> impl Iterator<Item = (&CommentId, &thread::Comment<CodeLocation>)> {
        self.discussion.comments()
    }

    /// Reviews of this revision's changes (one per actor).
    pub fn reviews(&self) -> impl DoubleEndedIterator<Item = (&PublicKey, &Review)> {
        self.reviews.iter()
    }

    /// Get a review by author.
    pub fn review_by(&self, author: &ActorId) -> Option<&Review> {
        self.reviews.get(author)
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

impl From<&State> for Status {
    fn from(value: &State) -> Self {
        match value {
            State::Draft => Self::Draft,
            State::Open { .. } => Self::Open,
            State::Archived => Self::Archived,
            State::Merged { .. } => Self::Merged,
        }
    }
}

/// A simplified enumeration of a [`State`] that can be used for
/// filtering purposes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    Draft,
    #[default]
    Open,
    Archived,
    Merged,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archived => write!(f, "archived"),
            Self::Draft => write!(f, "draft"),
            Self::Open => write!(f, "open"),
            Self::Merged => write!(f, "merged"),
        }
    }
}

/// A lifecycle operation, resulting in a new state.
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum Lifecycle {
    #[default]
    Open,
    Draft,
    Archived,
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

/// A patch review on a revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    /// Review identifier.
    pub(super) id: ReviewId,
    /// Review author.
    pub(super) author: Author,
    /// Review verdict.
    ///
    /// The verdict cannot be changed, since revisions are immutable.
    pub(super) verdict: Option<Verdict>,
    /// Review summary.
    ///
    /// Can be edited or set to `None`.
    pub(super) summary: Option<String>,
    /// Review comments.
    pub(super) comments: Thread<Comment<CodeLocation>>,
    /// Labels qualifying the review. For example if this review only looks at the
    /// concept or intention of the patch, it could have a "concept" label.
    pub(super) labels: Vec<Label>,
    /// Review timestamp.
    pub(super) timestamp: Timestamp,
}

impl Review {
    pub fn new(
        id: ReviewId,
        author: Author,
        verdict: Option<Verdict>,
        summary: Option<String>,
        labels: Vec<Label>,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            id,
            author,
            verdict,
            summary,
            comments: Thread::default(),
            labels,
            timestamp,
        }
    }

    /// Review identifier.
    pub fn id(&self) -> ReviewId {
        self.id
    }

    /// Review author.
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// Review verdict.
    pub fn verdict(&self) -> Option<Verdict> {
        self.verdict
    }

    /// Review inline code comments.
    pub fn comments(&self) -> impl DoubleEndedIterator<Item = (&EntryId, &Comment<CodeLocation>)> {
        self.comments.comments()
    }

    /// Review labels.
    pub fn labels(&self) -> impl Iterator<Item = &Label> {
        self.labels.iter()
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

impl<R: ReadRepository> store::Transaction<Patch, R> {
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
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::RevisionEdit {
            revision,
            description: description.to_string(),
            embeds,
        })
    }

    /// Redact the revision.
    pub fn redact(&mut self, revision: RevisionId) -> Result<(), store::Error> {
        self.push(Action::RevisionRedact { revision })
    }

    /// Start a patch revision discussion.
    pub fn thread<S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
    ) -> Result<(), store::Error> {
        self.push(Action::RevisionComment {
            revision,
            body: body.to_string(),
            reply_to: None,
            location: None,
            embeds: vec![],
        })
    }

    /// React on a patch revision.
    pub fn react(
        &mut self,
        revision: RevisionId,
        reaction: Reaction,
        location: Option<CodeLocation>,
        active: bool,
    ) -> Result<(), store::Error> {
        self.push(Action::RevisionReact {
            revision,
            reaction,
            location,
            active,
        })
    }

    /// Comment on a patch revision.
    pub fn comment<S: ToString>(
        &mut self,
        revision: RevisionId,
        body: S,
        reply_to: Option<CommentId>,
        location: Option<CodeLocation>,
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::RevisionComment {
            revision,
            body: body.to_string(),
            reply_to,
            location,
            embeds,
        })
    }

    /// Edit a comment on a patch revision.
    pub fn comment_edit<S: ToString>(
        &mut self,
        revision: RevisionId,
        comment: CommentId,
        body: S,
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::RevisionCommentEdit {
            revision,
            comment,
            body: body.to_string(),
            embeds,
        })
    }

    /// React a comment on a patch revision.
    pub fn comment_react(
        &mut self,
        revision: RevisionId,
        comment: CommentId,
        reaction: Reaction,
        active: bool,
    ) -> Result<(), store::Error> {
        self.push(Action::RevisionCommentReact {
            revision,
            comment,
            reaction,
            active,
        })
    }

    /// Redact a comment on a patch revision.
    pub fn comment_redact(
        &mut self,
        revision: RevisionId,
        comment: CommentId,
    ) -> Result<(), store::Error> {
        self.push(Action::RevisionCommentRedact { revision, comment })
    }

    /// Comment on a review.
    pub fn review_comment<S: ToString>(
        &mut self,
        review: ReviewId,
        body: S,
        location: Option<CodeLocation>,
        reply_to: Option<CommentId>,
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::ReviewComment {
            review,
            body: body.to_string(),
            location,
            reply_to,
            embeds,
        })
    }

    /// Resolve a review comment.
    pub fn review_comment_resolve(
        &mut self,
        review: ReviewId,
        comment: CommentId,
    ) -> Result<(), store::Error> {
        self.push(Action::ReviewCommentResolve { review, comment })
    }

    /// Unresolve a review comment.
    pub fn review_comment_unresolve(
        &mut self,
        review: ReviewId,
        comment: CommentId,
    ) -> Result<(), store::Error> {
        self.push(Action::ReviewCommentUnresolve { review, comment })
    }

    /// Edit review comment.
    pub fn edit_review_comment<S: ToString>(
        &mut self,
        review: ReviewId,
        comment: EntryId,
        body: S,
        embeds: Vec<Embed<Uri>>,
    ) -> Result<(), store::Error> {
        self.embed(embeds.clone())?;
        self.push(Action::ReviewCommentEdit {
            review,
            comment,
            body: body.to_string(),
            embeds,
        })
    }

    /// React to a review comment.
    pub fn react_review_comment(
        &mut self,
        review: ReviewId,
        comment: EntryId,
        reaction: Reaction,
        active: bool,
    ) -> Result<(), store::Error> {
        self.push(Action::ReviewCommentReact {
            review,
            comment,
            reaction,
            active,
        })
    }

    /// Redact a review comment.
    pub fn redact_review_comment(
        &mut self,
        review: ReviewId,
        comment: EntryId,
    ) -> Result<(), store::Error> {
        self.push(Action::ReviewCommentRedact { review, comment })
    }

    /// Review a patch revision.
    /// Does nothing if a review for that revision already exists by the author.
    pub fn review(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        summary: Option<String>,
        labels: Vec<Label>,
    ) -> Result<(), store::Error> {
        self.push(Action::Review {
            revision,
            summary,
            verdict,
            labels,
        })
    }

    /// Edit a review.
    pub fn review_edit(
        &mut self,
        review: ReviewId,
        verdict: Option<Verdict>,
        summary: Option<String>,
        labels: Vec<Label>,
    ) -> Result<(), store::Error> {
        self.push(Action::ReviewEdit {
            review,
            summary,
            verdict,
            labels,
        })
    }

    /// Redact a patch review.
    pub fn redact_review(&mut self, review: ReviewId) -> Result<(), store::Error> {
        self.push(Action::ReviewRedact { review })
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
            resolves: BTreeSet::new(),
        })
    }

    /// Lifecycle a patch.
    pub fn lifecycle(&mut self, state: Lifecycle) -> Result<(), store::Error> {
        self.push(Action::Lifecycle { state })
    }

    /// Assign a patch.
    pub fn assign(&mut self, assignees: BTreeSet<Did>) -> Result<(), store::Error> {
        self.push(Action::Assign { assignees })
    }

    /// Label a patch.
    pub fn label(&mut self, labels: impl IntoIterator<Item = Label>) -> Result<(), store::Error> {
        self.push(Action::Label {
            labels: labels.into_iter().collect(),
        })
    }
}

pub struct PatchMut<'a, 'g, R, C> {
    pub id: ObjectId,

    patch: Patch,
    store: &'g mut Patches<'a, R>,
    cache: &'g mut C,
}

impl<'a, 'g, R, C> PatchMut<'a, 'g, R, C>
where
    C: cob::cache::Update<Patch>,
    R: ReadRepository + SignRepository + cob::Store,
{
    pub fn new(id: ObjectId, patch: Patch, cache: &'g mut Cache<Patches<'a, R>, C>) -> Self {
        Self {
            id,
            patch,
            store: &mut cache.store,
            cache: &mut cache.cache,
        }
    }

    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Reload the patch data from storage.
    pub fn reload(&mut self) -> Result<(), store::Error> {
        self.patch = self
            .store
            .get(&self.id)?
            .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), self.id))?;

        Ok(())
    }

    pub fn transaction<G, F>(
        &mut self,
        message: &str,
        signer: &G,
        operations: F,
    ) -> Result<EntryId, Error>
    where
        G: Signer,
        F: FnOnce(&mut Transaction<Patch, R>) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::default();
        operations(&mut tx)?;

        let (patch, commit) = tx.commit(message, self.id, &mut self.store.raw, signer)?;
        self.cache
            .update(&self.store.as_ref().id(), &self.id, &patch)
            .map_err(|e| Error::CacheUpdate {
                id: self.id,
                err: e.into(),
            })?;
        self.patch = patch;

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
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit revision", signer, |tx| {
            tx.edit_revision(revision, description, embeds.into_iter().collect())
        })
    }

    /// Redact a revision.
    pub fn redact<G: Signer>(
        &mut self,
        revision: RevisionId,
        signer: &G,
    ) -> Result<EntryId, Error> {
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
        location: Option<CodeLocation>,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Comment", signer, |tx| {
            tx.comment(
                revision,
                body,
                reply_to,
                location,
                embeds.into_iter().collect(),
            )
        })
    }

    /// React on a patch revision.
    pub fn react<G: Signer>(
        &mut self,
        revision: RevisionId,
        reaction: Reaction,
        location: Option<CodeLocation>,
        active: bool,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("React", signer, |tx| {
            tx.react(revision, reaction, location, active)
        })
    }

    /// Edit a comment on a patch revision.
    pub fn comment_edit<G: Signer, S: ToString>(
        &mut self,
        revision: RevisionId,
        comment: CommentId,
        body: S,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit comment", signer, |tx| {
            tx.comment_edit(revision, comment, body, embeds.into_iter().collect())
        })
    }

    /// React to a comment on a patch revision.
    pub fn comment_react<G: Signer>(
        &mut self,
        revision: RevisionId,
        comment: CommentId,
        reaction: Reaction,
        active: bool,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("React comment", signer, |tx| {
            tx.comment_react(revision, comment, reaction, active)
        })
    }

    /// Redact a comment on a patch revision.
    pub fn comment_redact<G: Signer>(
        &mut self,
        revision: RevisionId,
        comment: CommentId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Redact comment", signer, |tx| {
            tx.comment_redact(revision, comment)
        })
    }

    /// Comment on a line of code as part of a review.
    pub fn review_comment<G: Signer, S: ToString>(
        &mut self,
        review: ReviewId,
        body: S,
        location: Option<CodeLocation>,
        reply_to: Option<CommentId>,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Review comment", signer, |tx| {
            tx.review_comment(
                review,
                body,
                location,
                reply_to,
                embeds.into_iter().collect(),
            )
        })
    }

    /// Edit review comment.
    pub fn edit_review_comment<G: Signer, S: ToString>(
        &mut self,
        review: ReviewId,
        comment: EntryId,
        body: S,
        embeds: impl IntoIterator<Item = Embed<Uri>>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit review comment", signer, |tx| {
            tx.edit_review_comment(review, comment, body, embeds.into_iter().collect())
        })
    }

    /// React to a review comment.
    pub fn react_review_comment<G: Signer>(
        &mut self,
        review: ReviewId,
        comment: EntryId,
        reaction: Reaction,
        active: bool,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("React to review comment", signer, |tx| {
            tx.react_review_comment(review, comment, reaction, active)
        })
    }

    /// React to a review comment.
    pub fn redact_review_comment<G: Signer>(
        &mut self,
        review: ReviewId,
        comment: EntryId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Redact review comment", signer, |tx| {
            tx.redact_review_comment(review, comment)
        })
    }

    /// Review a patch revision.
    pub fn review<G: Signer>(
        &mut self,
        revision: RevisionId,
        verdict: Option<Verdict>,
        summary: Option<String>,
        labels: Vec<Label>,
        signer: &G,
    ) -> Result<ReviewId, Error> {
        if verdict.is_none() && summary.is_none() {
            return Err(Error::EmptyReview);
        }
        self.transaction("Review", signer, |tx| {
            tx.review(revision, verdict, summary, labels)
        })
        .map(ReviewId)
    }

    /// Edit a review.
    pub fn review_edit<G: Signer>(
        &mut self,
        review: ReviewId,
        verdict: Option<Verdict>,
        summary: Option<String>,
        labels: Vec<Label>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Edit review", signer, |tx| {
            tx.review_edit(review, verdict, summary, labels)
        })
    }

    /// Redact a patch review.
    pub fn redact_review<G: Signer>(
        &mut self,
        review: ReviewId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Redact review", signer, |tx| tx.redact_review(review))
    }

    /// Resolve a patch review comment.
    pub fn resolve_review_comment<G: Signer>(
        &mut self,
        review: ReviewId,
        comment: CommentId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Resolve review comment", signer, |tx| {
            tx.review_comment_resolve(review, comment)
        })
    }

    /// Unresolve a patch review comment.
    pub fn unresolve_review_comment<G: Signer>(
        &mut self,
        review: ReviewId,
        comment: CommentId,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Unresolve review comment", signer, |tx| {
            tx.review_comment_unresolve(review, comment)
        })
    }

    /// Merge a patch revision.
    pub fn merge<G: Signer>(
        &mut self,
        revision: RevisionId,
        commit: git::Oid,
        signer: &G,
    ) -> Result<Merged<R>, Error> {
        // TODO: Don't allow merging the same revision twice?
        let entry = self.transaction("Merge revision", signer, |tx| tx.merge(revision, commit))?;

        Ok(Merged {
            entry,
            patch: self.id,
            stored: self.store.as_ref(),
        })
    }

    /// Update a patch with a new revision.
    pub fn update<G: Signer>(
        &mut self,
        description: impl ToString,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        signer: &G,
    ) -> Result<RevisionId, Error> {
        self.transaction("Add revision", signer, |tx| {
            tx.revision(description, base, oid)
        })
        .map(RevisionId)
    }

    /// Lifecycle a patch.
    pub fn lifecycle<G: Signer>(&mut self, state: Lifecycle, signer: &G) -> Result<EntryId, Error> {
        self.transaction("Lifecycle", signer, |tx| tx.lifecycle(state))
    }

    /// Assign a patch.
    pub fn assign<G: Signer>(
        &mut self,
        assignees: BTreeSet<Did>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Assign", signer, |tx| tx.assign(assignees))
    }

    /// Archive a patch.
    pub fn archive<G: Signer>(&mut self, signer: &G) -> Result<bool, Error> {
        self.lifecycle(Lifecycle::Archived, signer)?;

        Ok(true)
    }

    /// Mark an archived patch as ready to be reviewed again.
    /// Returns `false` if the patch was not archived.
    pub fn unarchive<G: Signer>(&mut self, signer: &G) -> Result<bool, Error> {
        if !self.is_archived() {
            return Ok(false);
        }
        self.lifecycle(Lifecycle::Open, signer)?;

        Ok(true)
    }

    /// Mark a patch as ready to be reviewed.
    /// Returns `false` if the patch was not a draft.
    pub fn ready<G: Signer>(&mut self, signer: &G) -> Result<bool, Error> {
        if !self.is_draft() {
            return Ok(false);
        }
        self.lifecycle(Lifecycle::Open, signer)?;

        Ok(true)
    }

    /// Mark an open patch as a draft.
    /// Returns `false` if the patch was not open and free of merges.
    pub fn unready<G: Signer>(&mut self, signer: &G) -> Result<bool, Error> {
        if !matches!(self.state(), State::Open { conflicts } if conflicts.is_empty()) {
            return Ok(false);
        }
        self.lifecycle(Lifecycle::Draft, signer)?;

        Ok(true)
    }

    /// Label a patch.
    pub fn label<G: Signer>(
        &mut self,
        labels: impl IntoIterator<Item = Label>,
        signer: &G,
    ) -> Result<EntryId, Error> {
        self.transaction("Label", signer, |tx| tx.label(labels))
    }
}

impl<R, C> Deref for PatchMut<'_, '_, R, C> {
    type Target = Patch;

    fn deref(&self) -> &Self::Target {
        &self.patch
    }
}

/// Detailed information on patch states
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCounts {
    pub open: usize,
    pub draft: usize,
    pub archived: usize,
    pub merged: usize,
}

impl PatchCounts {
    /// Total count.
    pub fn total(&self) -> usize {
        self.open + self.draft + self.archived + self.merged
    }
}

/// Result of looking up a `Patch`'s `Revision`.
///
/// See [`Patches::find_by_revision`].
#[derive(Debug, PartialEq, Eq)]
pub struct ByRevision {
    pub id: PatchId,
    pub patch: Patch,
    pub revision_id: RevisionId,
    pub revision: Revision,
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

impl<R> HasRepoId for Patches<'_, R>
where
    R: ReadRepository,
{
    fn rid(&self) -> RepoId {
        self.as_ref().id()
    }
}

impl<'a, R> Patches<'a, R>
where
    R: ReadRepository + cob::Store,
{
    /// Open a patches store.
    pub fn open(repository: &'a R) -> Result<Self, RepositoryError> {
        let identity = repository.identity_head()?;
        let raw = store::Store::open(repository)?.identity(identity);

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
    pub fn find_by_revision(&self, revision: &RevisionId) -> Result<Option<ByRevision>, Error> {
        // Revision may be the patch's first, making it have the same ID.
        let p_id = ObjectId::from(revision.into_inner());
        if let Some(p) = self.get(&p_id)? {
            return Ok(p.revision(revision).map(|r| ByRevision {
                id: p_id,
                patch: p.clone(),
                revision_id: *revision,
                revision: r.clone(),
            }));
        }
        let result = self
            .all()?
            .filter_map(|result| result.ok())
            .find_map(|(p_id, p)| {
                p.revision(revision).map(|r| ByRevision {
                    id: p_id,
                    patch: p.clone(),
                    revision_id: *revision,
                    revision: r.clone(),
                })
            });

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
    ) -> Result<impl Iterator<Item = (PatchId, Patch)> + 'b, Error> {
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
    pub fn create<'g, C, G>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
        cache: &'g mut C,
        signer: &G,
    ) -> Result<PatchMut<'a, 'g, R, C>, Error>
    where
        C: cob::cache::Update<Patch>,
        G: Signer,
    {
        self._create(
            title,
            description,
            target,
            base,
            oid,
            labels,
            Lifecycle::default(),
            cache,
            signer,
        )
    }

    /// Draft a patch. This patch will be created in a [`State::Draft`] state.
    pub fn draft<'g, C, G: Signer>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
        cache: &'g mut C,
        signer: &G,
    ) -> Result<PatchMut<'a, 'g, R, C>, Error>
    where
        C: cob::cache::Update<Patch>,
    {
        self._create(
            title,
            description,
            target,
            base,
            oid,
            labels,
            Lifecycle::Draft,
            cache,
            signer,
        )
    }

    /// Get a patch mutably.
    pub fn get_mut<'g, C>(
        &'g mut self,
        id: &ObjectId,
        cache: &'g mut C,
    ) -> Result<PatchMut<'a, 'g, R, C>, store::Error> {
        let patch = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(PatchMut {
            id: *id,
            patch,
            store: self,
            cache,
        })
    }

    /// Create a patch. This is an internal function used by `create` and `draft`.
    fn _create<'g, C, G: Signer>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
        state: Lifecycle,
        cache: &'g mut C,
        signer: &G,
    ) -> Result<PatchMut<'a, 'g, R, C>, Error>
    where
        C: cob::cache::Update<Patch>,
    {
        let (id, patch) = Transaction::initial("Create patch", &mut self.raw, signer, |tx, _| {
            tx.revision(description, base, oid)?;
            tx.edit(title, target)?;

            if !labels.is_empty() {
                tx.label(labels.to_owned())?;
            }
            if state != Lifecycle::default() {
                tx.lifecycle(state)?;
            }
            Ok(())
        })?;
        cache
            .update(&self.raw.as_ref().id(), &id, &patch)
            .map_err(|e| Error::CacheUpdate { id, err: e.into() })?;

        Ok(PatchMut {
            id,
            patch,
            store: self,
            cache,
        })
    }
}

/// Models a comparison between two commit ranges,
/// commonly obtained from two revisions (likely of the same patch).
/// This can be used to generate a `git range-diff` command.
/// See <https://git-scm.com/docs/git-range-diff>.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct RangeDiff {
    old: (git::Oid, git::Oid),
    new: (git::Oid, git::Oid),
}

impl RangeDiff {
    const COMMAND: &str = "git";
    const SUBCOMMAND: &str = "range-diff";

    pub fn new(old: &Revision, new: &Revision) -> Self {
        Self {
            old: old.range(),
            new: new.range(),
        }
    }

    pub fn to_command(&self) -> String {
        let range = if self.has_same_base() {
            format!("{} {} {}", self.old.0, self.old.1, self.new.1)
        } else {
            format!(
                "{}..{} {}..{}",
                self.old.0, self.old.1, self.new.0, self.new.1,
            )
        };

        Self::COMMAND.to_string() + " " + Self::SUBCOMMAND + " " + &range
    }

    fn has_same_base(&self) -> bool {
        self.old.0 == self.new.0
    }
}

impl From<RangeDiff> for std::process::Command {
    fn from(range_diff: RangeDiff) -> Self {
        let mut command = std::process::Command::new(RangeDiff::COMMAND);

        command.arg(RangeDiff::SUBCOMMAND);

        if range_diff.has_same_base() {
            command.args([
                range_diff.old.0.to_string(),
                range_diff.old.1.to_string(),
                range_diff.new.1.to_string(),
            ]);
        } else {
            command.args([
                format!("{}..{}", range_diff.old.0, range_diff.old.1),
                format!("{}..{}", range_diff.new.0, range_diff.new.1),
            ]);
        }
        command
    }
}

/// Helpers for de/serialization of patch data types.
mod ser {
    use std::collections::{BTreeMap, BTreeSet};

    use serde::ser::SerializeSeq;

    use crate::cob::{thread::Reactions, ActorId, CodeLocation};

    /// Serialize a `Revision`'s reaction as an object containing the
    /// `location`, `emoji`, and all `authors` that have performed the
    /// same reaction.
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reaction {
        location: Option<CodeLocation>,
        emoji: super::Reaction,
        authors: Vec<ActorId>,
    }

    impl Reaction {
        fn as_revision_reactions(
            reactions: Vec<Reaction>,
        ) -> BTreeMap<Option<CodeLocation>, Reactions> {
            reactions.into_iter().fold(
                BTreeMap::<Option<CodeLocation>, Reactions>::new(),
                |mut reactions,
                 Reaction {
                     location,
                     emoji,
                     authors,
                 }| {
                    let mut inner = authors
                        .into_iter()
                        .map(|author| (author, emoji))
                        .collect::<BTreeSet<_>>();
                    let entry = reactions.entry(location).or_default();
                    entry.append(&mut inner);
                    reactions
                },
            )
        }
    }

    /// Helper to serialize a `Revision`'s reactions, since
    /// `CodeLocation` cannot be a key for a JSON object.
    ///
    /// The set `reactions` are first turned into a set of
    /// [`Reaction`]s and then serialized via a `Vec`.
    pub fn serialize_reactions<S>(
        reactions: &BTreeMap<Option<CodeLocation>, Reactions>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let reactions = reactions
            .iter()
            .flat_map(|(location, reaction)| {
                let reactions = reaction.iter().fold(
                    BTreeMap::new(),
                    |mut acc: BTreeMap<&super::Reaction, Vec<_>>, (author, emoji)| {
                        acc.entry(emoji).or_default().push(*author);
                        acc
                    },
                );
                reactions
                    .into_iter()
                    .map(|(emoji, authors)| Reaction {
                        location: location.clone(),
                        emoji: *emoji,
                        authors,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut s = serializer.serialize_seq(Some(reactions.len()))?;
        for r in &reactions {
            s.serialize_element(r)?;
        }
        s.end()
    }

    /// Helper to deserialize a `Revision`'s reactions, the inverse of
    /// `serialize_reactions`.
    ///
    /// The `Vec` of [`Reaction`]s are deserialized and converted to a
    /// `BTreeMap<Option<CodeLocation>, Reactions>`.
    pub fn deserialize_reactions<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<Option<CodeLocation>, Reactions>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReactionsVisitor;

        impl<'de> serde::de::Visitor<'de> for ReactionsVisitor {
            type Value = Vec<Reaction>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a reaction of the form {'location', 'emoji', 'authors'}")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut reactions = Vec::new();
                while let Some(reaction) = seq.next_element()? {
                    reactions.push(reaction);
                }
                Ok(reactions)
            }
        }

        let reactions = deserializer.deserialize_seq(ReactionsVisitor)?;
        Ok(Reaction::as_revision_reactions(reactions))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::vec;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cob::common::CodeRange;
    use crate::cob::test::Actor;
    use crate::crypto::test::signer::MockSigner;
    use crate::git::canonical::rules::RawRules;
    use crate::identity;
    use crate::patch::cache::Patches as _;
    use crate::profile::env;
    use crate::test;
    use crate::test::arbitrary;
    use crate::test::arbitrary::gen;
    use crate::test::storage::MockRepository;

    use cob::migrate;

    #[test]
    fn test_json_serialization() {
        let edit = Action::Label {
            labels: BTreeSet::new(),
        };
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            String::from(r#"{"type":"label","labels":[]}"#)
        );
    }

    #[test]
    fn test_reactions_json_serialization() {
        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        struct TestReactions {
            #[serde(
                serialize_with = "super::ser::serialize_reactions",
                deserialize_with = "super::ser::deserialize_reactions"
            )]
            inner: BTreeMap<Option<CodeLocation>, Reactions>,
        }

        let reactions = TestReactions {
            inner: [(
                None,
                [
                    (
                        "z6Mkk7oqY4pPxhMmGEotDYsFo97vhCj85BLY1H256HrJmjN8"
                            .parse()
                            .unwrap(),
                        Reaction::new('🚀').unwrap(),
                    ),
                    (
                        "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
                            .parse()
                            .unwrap(),
                        Reaction::new('🙏').unwrap(),
                    ),
                ]
                .into_iter()
                .collect(),
            )]
            .into_iter()
            .collect(),
        };

        assert_eq!(
            reactions,
            serde_json::from_str(&serde_json::to_string(&reactions).unwrap()).unwrap()
        );
    }

    #[test]
    fn test_patch_create_and_get() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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

        let ByRevision { id, .. } = patches.find_by_revision(&rev_id).unwrap().unwrap();
        assert_eq!(id, patch_id);
    }

    #[test]
    fn test_patch_discussion() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
                .comment(revision_id, "patch comment", None, None, [], &alice.signer)
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
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
        let _merge = patch.merge(rid, branch.base, &alice.signer).unwrap();
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
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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

        let (revision_id, _) = patch.latest();
        let review_id = patch
            .review(
                revision_id,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                &alice.signer,
            )
            .unwrap();

        let id = patch.id;
        let mut patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest();
        assert_eq!(revision.reviews.len(), 1);

        let review = revision.review_by(alice.signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.summary(), Some("LGTM"));

        patch.redact_review(review_id, &alice.signer).unwrap();
        patch.reload().unwrap();

        let (_, revision) = patch.latest();
        assert_eq!(revision.reviews().count(), 0);

        // This is fine, redacting an already-redacted review is a no-op.
        patch.redact_review(review_id, &alice.signer).unwrap();
        // If the review never existed, it's an error.
        patch
            .redact_review(ReviewId(arbitrary::entry_id()), &alice.signer)
            .unwrap_err();
    }

    #[test]
    fn test_patch_review_revision_redact() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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

        let update = checkout.branch_with([("README", b"Hello Radicle!")]);
        let updated = patch
            .update("I've made changes.", branch.base, update.oid, &alice.signer)
            .unwrap();

        // It's fine to redact a review from a redacted revision.
        let review = patch
            .review(updated, Some(Verdict::Accept), None, vec![], &alice.signer)
            .unwrap();
        patch.redact(updated, &alice.signer).unwrap();
        patch.redact_review(review, &alice.signer).unwrap();
    }

    #[test]
    fn test_revision_review_merge_redacted() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut alice = Actor::new(MockSigner::default());
        let rid = gen::<RepoId>(1);
        let doc = RawDoc::new(
            gen::<Project>(1),
            vec![alice.did()],
            1,
            RawRules::default(),
            identity::Visibility::Public,
        )
        .verified()
        .unwrap();
        let repo = MockRepository::new(rid, doc);

        let a1 = alice.op::<Patch>([
            Action::Revision {
                description: String::new(),
                base,
                oid,
                resolves: Default::default(),
            },
            Action::Edit {
                title: String::from("My patch"),
                target: MergeTarget::Delegates,
            },
        ]);
        let a2 = alice.op::<Patch>([Action::Revision {
            description: String::from("Second revision"),
            base,
            oid,
            resolves: Default::default(),
        }]);
        let a3 = alice.op::<Patch>([Action::RevisionRedact {
            revision: RevisionId(a2.id()),
        }]);
        let a4 = alice.op::<Patch>([Action::Review {
            revision: RevisionId(a2.id()),
            summary: None,
            verdict: Some(Verdict::Accept),
            labels: vec![],
        }]);
        let a5 = alice.op::<Patch>([Action::Merge {
            revision: RevisionId(a2.id()),
            commit: oid,
        }]);

        let mut patch = Patch::from_ops([a1, a2], &repo).unwrap();
        assert_eq!(patch.revisions().count(), 2);

        patch.op(a3, [], &repo).unwrap();
        assert_eq!(patch.revisions().count(), 1);

        patch.op(a4, [], &repo).unwrap();
        patch.op(a5, [], &repo).unwrap();
    }

    #[test]
    fn test_revision_edit_redact() {
        let base = arbitrary::oid();
        let oid = arbitrary::oid();
        let repo = gen::<MockRepository>(1);
        let time = env::local_time();
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let mut h0: cob::test::HistoryBuilder<Patch> = cob::test::history(
            &[
                Action::Revision {
                    description: String::from("Original"),
                    base,
                    oid,
                    resolves: Default::default(),
                },
                Action::Edit {
                    title: String::from("Some patch"),
                    target: MergeTarget::Delegates,
                },
            ],
            time.into(),
            &alice,
        );
        let r1 = h0.commit(
            &Action::Revision {
                description: String::from("New"),
                base,
                oid,
                resolves: Default::default(),
            },
            &alice,
        );
        let patch = Patch::from_history(&h0, &repo).unwrap();
        assert_eq!(patch.revisions().count(), 2);

        let mut h1 = h0.clone();
        h1.commit(
            &Action::RevisionRedact {
                revision: RevisionId(r1),
            },
            &alice,
        );

        let mut h2 = h0.clone();
        h2.commit(
            &Action::RevisionEdit {
                revision: RevisionId(*h0.root().id()),
                description: String::from("Edited"),
                embeds: Vec::default(),
            },
            &bob,
        );

        h0.merge(h1);
        h0.merge(h2);

        let patch = Patch::from_history(&h0, &repo).unwrap();
        assert_eq!(patch.revisions().count(), 1);
    }

    #[test]
    fn test_revision_reaction() {
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let mut alice = Actor::new(MockSigner::default());
        let repo = gen::<MockRepository>(1);
        let reaction = Reaction::new('👍').expect("failed to create a reaction");

        let a1 = alice.op::<Patch>([
            Action::Revision {
                description: String::new(),
                base,
                oid,
                resolves: Default::default(),
            },
            Action::Edit {
                title: String::from("My patch"),
                target: MergeTarget::Delegates,
            },
        ]);
        let a2 = alice.op::<Patch>([Action::RevisionReact {
            revision: RevisionId(a1.id()),
            location: None,
            reaction,
            active: true,
        }]);
        let patch = Patch::from_ops([a1, a2], &repo).unwrap();

        let (_, r1) = patch.revisions().next().unwrap();
        assert!(!r1.reactions.is_empty());

        let mut reactions = r1.reactions.get(&None).unwrap().clone();
        assert!(!reactions.is_empty());

        let (_, first_reaction) = reactions.pop_first().unwrap();
        assert_eq!(first_reaction, reaction);
    }

    #[test]
    fn test_patch_review_edit() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
        let review = patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("LGTM".to_owned()),
                vec![],
                &alice.signer,
            )
            .unwrap();
        patch
            .review_edit(
                review,
                Some(Verdict::Reject),
                Some("Whoops!".to_owned()),
                vec![],
                &alice.signer,
            )
            .unwrap(); // Overwrite the comment.

        let (_, revision) = patch.latest();
        let review = revision.review_by(alice.signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.summary(), Some("Whoops!"));
    }

    #[test]
    fn test_patch_review_duplicate() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
            .review(rid, Some(Verdict::Accept), None, vec![], &alice.signer)
            .unwrap();
        patch
            .review(rid, Some(Verdict::Reject), None, vec![], &alice.signer)
            .unwrap(); // This review is ignored, since there is already a review by this author.

        let (_, revision) = patch.latest();
        let review = revision.review_by(alice.signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Accept));
    }

    #[test]
    fn test_patch_review_edit_comment() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
        let review = patch
            .review(rid, Some(Verdict::Accept), None, vec![], &alice.signer)
            .unwrap();
        patch
            .review_comment(review, "First comment!", None, None, [], &alice.signer)
            .unwrap();

        let _review = patch
            .review_edit(review, Some(Verdict::Reject), None, vec![], &alice.signer)
            .unwrap();
        patch
            .review_comment(review, "Second comment!", None, None, [], &alice.signer)
            .unwrap();

        let (_, revision) = patch.latest();
        let review = revision.review_by(alice.signer.public_key()).unwrap();
        assert_eq!(review.verdict(), Some(Verdict::Reject));
        assert_eq!(review.comments().count(), 2);
        assert_eq!(review.comments().nth(0).unwrap().1.body(), "First comment!");
        assert_eq!(
            review.comments().nth(1).unwrap().1.body(),
            "Second comment!"
        );
    }

    #[test]
    fn test_patch_review_comment() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
        let location = CodeLocation {
            commit: branch.oid,
            path: PathBuf::from_str("README").unwrap(),
            old: None,
            new: Some(CodeRange::Lines { range: 5..8 }),
        };
        let review = patch
            .review(rid, Some(Verdict::Accept), None, vec![], &alice.signer)
            .unwrap();
        patch
            .review_comment(
                review,
                "I like these lines of code",
                Some(location.clone()),
                None,
                [],
                &alice.signer,
            )
            .unwrap();

        let (_, revision) = patch.latest();
        let review = revision.review_by(alice.signer.public_key()).unwrap();
        let (_, comment) = review.comments().next().unwrap();

        assert_eq!(comment.body(), "I like these lines of code");
        assert_eq!(comment.location(), Some(&location));
    }

    #[test]
    fn test_patch_review_remove_summary() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = Cache::no_cache(&*alice.repo).unwrap();
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
        let review = patch
            .review(
                rid,
                Some(Verdict::Accept),
                Some("Nah".to_owned()),
                vec![],
                &alice.signer,
            )
            .unwrap();
        patch
            .review_edit(review, Some(Verdict::Accept), None, vec![], &alice.signer)
            .unwrap();

        let id = patch.id;
        let patch = patches.get_mut(&id).unwrap();
        let (_, revision) = patch.latest();
        let review = revision.review_by(alice.signer.public_key()).unwrap();

        assert_eq!(review.verdict(), Some(Verdict::Accept));
        assert_eq!(review.summary(), None);
    }

    #[test]
    fn test_patch_update() {
        let alice = test::setup::NodeWithRepo::default();
        let checkout = alice.repo.checkout();
        let branch = checkout.branch_with([("README", b"Hello World!")]);
        let mut patches = {
            let path = alice.tmp.path().join("cobs.db");
            let mut db = cob::cache::Store::open(path).unwrap();
            let store = cob::patch::Patches::open(&*alice.repo).unwrap();

            db.migrate(migrate::ignore).unwrap();
            cob::patch::Cache::open(store, db)
        };
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
        let mut patches = Cache::no_cache(&*repo).unwrap();
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
        assert_eq!(patch.latest().0, RevisionId(*patch_id));
        assert_eq!(patch.revisions().count(), 1);

        // The patch's root must always exist.
        assert_eq!(patch.latest(), patch.root());
        assert!(patch.redact(patch.latest().0, &alice.signer).is_err());
    }

    #[test]
    fn test_json() {
        use serde_json::json;

        assert_eq!(
            serde_json::to_value(Action::Lifecycle {
                state: Lifecycle::Draft
            })
            .unwrap(),
            json!({
                "type": "lifecycle",
                "state": { "status": "draft" }
            })
        );

        let revision = RevisionId(arbitrary::entry_id());
        assert_eq!(
            serde_json::to_value(Action::Review {
                revision,
                summary: None,
                verdict: None,
                labels: vec![],
            })
            .unwrap(),
            json!({
                "type": "review",
                "revision": revision,
            })
        );

        assert_eq!(
            serde_json::to_value(CodeRange::Lines { range: 4..8 }).unwrap(),
            json!({
                "type": "lines",
                "range": { "start": 4, "end": 8 },
            })
        );
    }
}
