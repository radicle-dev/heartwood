#![allow(clippy::too_many_arguments)]
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt;
use std::ops::{ControlFlow, Deref, RangeInclusive};
use std::str::FromStr;
use std::sync::Arc;

use automerge::transaction::Transactable;
use automerge::{Automerge, AutomergeError, ObjType, ScalarValue, Value};
use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::cob::doc::{Document, DocumentError};
use crate::cob::shared;
use crate::cob::shared::*;
use crate::cob::store::Error;
use crate::cob::store::{FromHistory, Store};
use crate::cob::transaction::{Transaction, TransactionError};
use crate::cob::value::{FromValue, ValueError};
use crate::cob::{Contents, History, ObjectId, TypeName};
use crate::git;
use crate::prelude::*;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.patch").expect("type name is valid"));

/// Identifier for a patch.
pub type PatchId = ObjectId;

/// Unique identifier for a patch revision.
pub type RevisionId = uuid::Uuid;

/// Index of a revision in the revisions list.
pub type RevisionIx = usize;

/// Where a patch is intended to be merged.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeTarget {
    /// Intended for the default branch of the project delegates.
    /// Note that if the delegations change while the patch is open,
    /// this will always mean whatever the "current" delegation set is.
    #[default]
    Delegates,
}

impl From<MergeTarget> for ScalarValue {
    fn from(target: MergeTarget) -> Self {
        match target {
            MergeTarget::Delegates => ScalarValue::from("delegates"),
        }
    }
}

impl<'a> FromValue<'a> for MergeTarget {
    fn from_value(value: Value<'a>) -> Result<Self, ValueError> {
        let state = value.to_str().ok_or(ValueError::InvalidType)?;

        match state {
            "delegates" => Ok(Self::Delegates),
            _ => Err(ValueError::InvalidValue(value.to_string())),
        }
    }
}

/// A patch to a repository.
#[derive(Debug, Clone, Serialize)]
pub struct Patch<T = ()>
where
    T: Clone,
{
    /// Author of the patch.
    pub author: Author,
    /// Title of the patch.
    pub title: String,
    /// Current state of the patch.
    pub state: State,
    /// Target this patch is meant to be merged in.
    pub target: MergeTarget,
    /// Labels associated with the patch.
    pub labels: HashSet<Label>,
    /// List of patch revisions. The initial changeset is part of the
    /// first revision.
    pub revisions: NonEmpty<Revision<T>>,
    /// Patch creation time.
    pub timestamp: Timestamp,
}

impl Patch {
    pub fn head(&self) -> &git::Oid {
        &self.revisions.last().oid
    }

    pub fn version(&self) -> RevisionIx {
        self.revisions.len() - 1
    }

    pub fn latest(&self) -> (RevisionIx, &Revision) {
        let version = self.version();
        let revision = &self.revisions[version];

        (version, revision)
    }

    pub fn is_proposed(&self) -> bool {
        matches!(self.state, State::Proposed)
    }

    pub fn is_archived(&self) -> bool {
        matches!(self.state, State::Archived)
    }

    pub fn description(&self) -> &str {
        self.latest().1.description()
    }
}

impl FromHistory for Patch {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }

    fn from_history(history: &History) -> Result<Self, Error> {
        Patch::try_from(history)
    }
}

impl TryFrom<Document<'_>> for Patch {
    type Error = DocumentError;

    fn try_from(doc: Document) -> Result<Self, Self::Error> {
        let obj_id = doc.get_id(automerge::ObjId::Root, "patch")?;
        let title = doc.get(&obj_id, "title")?;
        let author = doc.get(&obj_id, "author")?;
        let state = doc.get(&obj_id, "state")?;
        let target = doc.get(&obj_id, "target")?;
        let timestamp = doc.get(&obj_id, "timestamp")?;
        let revisions = doc.list(&obj_id, "revisions", lookup::revision)?;
        let labels: HashSet<Label> = doc.keys(&obj_id, "labels")?;
        let revisions =
            NonEmpty::from_vec(revisions).ok_or(DocumentError::EmptyList("revisions"))?;
        let author: Author = Author::new(author);

        Ok(Self {
            author,
            title,
            state,
            target,
            labels,
            revisions,
            timestamp,
        })
    }
}

impl TryFrom<&History> for Patch {
    type Error = Error;

    fn try_from(history: &History) -> Result<Self, Self::Error> {
        let doc = history.traverse(Automerge::new(), |mut doc, entry| {
            match entry.contents() {
                Contents::Automerge(bytes) => {
                    match automerge::Change::from_bytes(bytes.clone()) {
                        Ok(change) => {
                            doc.apply_changes([change]).ok();
                        }
                        Err(_err) => {
                            // Ignore
                        }
                    }
                }
            }
            ControlFlow::Continue(doc)
        });
        let patch = Patch::try_from(Document::new(&doc))?;

        Ok(patch)
    }
}

pub struct PatchStore<'a> {
    store: Store<'a, Patch>,
}

impl<'a> Deref for PatchStore<'a> {
    type Target = Store<'a, Patch>;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl<'a> PatchStore<'a> {
    /// Create a new patch store.
    pub fn new(store: Store<'a, Patch>) -> Self {
        Self { store }
    }

    /// Get a patch by id.
    pub fn get(&self, id: &ObjectId) -> Result<Option<Patch>, Error> {
        self.store.get(id)
    }

    /// Create a patch.
    pub fn create<G: Signer>(
        &self,
        title: &str,
        description: &str,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
        signer: &G,
    ) -> Result<PatchId, Error> {
        let author = self.author();
        let timestamp = Timestamp::now();
        let revision = Revision::new(
            author.clone(),
            base.into(),
            oid.into(),
            description.to_owned(),
            timestamp,
        );
        let contents = events::create(&author, title, &revision, target, timestamp, labels)?;
        let cob = self.store.create("Create patch", contents, signer)?;

        Ok(*cob.id())
    }

    /// Comment on a patch.
    pub fn comment<G: Signer>(
        &self,
        patch_id: &PatchId,
        revision_ix: RevisionIx,
        body: &str,
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.author();
        let mut patch = self.store.get_raw(patch_id)?;
        let timestamp = Timestamp::now();
        let changes = events::comment(&mut patch, revision_ix, &author, body, timestamp)?;

        self.store
            .update(*patch_id, "Add comment", changes, signer)?;

        Ok(())
    }

    /// Update a patch with new code. Creates a new revision.
    pub fn update<G: Signer>(
        &self,
        patch_id: &PatchId,
        comment: impl ToString,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        signer: &G,
    ) -> Result<RevisionIx, Error> {
        let author = self.author();
        let timestamp = Timestamp::now();
        let revision = Revision::new(
            author,
            base.into(),
            oid.into(),
            comment.to_string(),
            timestamp,
        );

        let mut patch = self.get_raw(patch_id)?;
        let (revision_ix, changes) = events::update(&mut patch, revision)?;

        self.store
            .update(*patch_id, "Update patch", changes, signer)?;

        Ok(revision_ix)
    }

    /// Reply to a patch comment.
    pub fn reply<G: Signer>(
        &self,
        patch_id: &PatchId,
        revision_ix: RevisionIx,
        comment_id: CommentId,
        reply: &str,
        signer: &G,
    ) -> Result<(), Error> {
        let author = self.author();
        let mut patch = self.get_raw(patch_id)?;
        let changes = events::reply(
            &mut patch,
            revision_ix,
            comment_id,
            &author,
            reply,
            Timestamp::now(),
        )?;

        self.store.update(*patch_id, "Reply", changes, signer)?;

        Ok(())
    }

    /// Review a patch revision.
    pub fn review<G: Signer>(
        &self,
        patch_id: &PatchId,
        revision_ix: RevisionIx,
        verdict: Option<Verdict>,
        comment: impl Into<String>,
        inline: Vec<CodeComment>,
        signer: &G,
    ) -> Result<(), Error> {
        let timestamp = Timestamp::now();
        let review = Review::new(self.author(), verdict, comment, inline, timestamp);

        let mut patch = self.get_raw(patch_id)?;
        let (_, changes) = events::review(&mut patch, revision_ix, review)?;

        self.store
            .update(*patch_id, "Review patch", changes, signer)?;

        Ok(())
    }

    /// Merge a patch revision.
    pub fn merge<G: Signer>(
        &self,
        patch_id: &PatchId,
        revision_ix: RevisionIx,
        commit: git::Oid,
        signer: &G,
    ) -> Result<Merge, Error> {
        let timestamp = Timestamp::now();
        let merge = Merge {
            node: *signer.public_key(),
            commit,
            timestamp,
        };

        let mut patch = self.get_raw(patch_id)?;
        let changes = events::merge(&mut patch, revision_ix, &merge)?;

        self.store
            .update(*patch_id, "Merge revision", changes, signer)?;

        Ok(merge)
    }

    /// Get the patch count.
    pub fn count(&self) -> Result<usize, Error> {
        let cobs = self.store.list()?;

        Ok(cobs.len())
    }

    /// Get all patches for this project.
    pub fn all(&self) -> Result<Vec<(PatchId, Patch)>, Error> {
        let mut patches = self.store.list()?;
        patches.sort_by_key(|(_, p)| p.timestamp);

        Ok(patches)
    }

    /// Get proposed patches.
    pub fn proposed(&self) -> Result<impl Iterator<Item = (PatchId, Patch)>, Error> {
        let all = self.all()?;

        Ok(all.into_iter().filter(|(_, p)| p.is_proposed()))
    }

    /// Get patches proposed by the given key.
    pub fn proposed_by<'b>(
        &'b self,
        who: &'b PublicKey,
    ) -> Result<impl Iterator<Item = (PatchId, Patch)> + '_, Error> {
        Ok(self.proposed()?.filter(move |(_, p)| p.author.id() == who))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Draft,
    Proposed,
    Archived,
}

impl From<State> for ScalarValue {
    fn from(state: State) -> Self {
        match state {
            State::Proposed => ScalarValue::from("proposed"),
            State::Draft => ScalarValue::from("draft"),
            State::Archived => ScalarValue::from("archived"),
        }
    }
}

impl<'a> FromValue<'a> for State {
    fn from_value(value: Value<'a>) -> Result<Self, ValueError> {
        let state = value.to_str().ok_or(ValueError::InvalidType)?;

        match state {
            "proposed" => Ok(Self::Proposed),
            "draft" => Ok(Self::Draft),
            "archived" => Ok(Self::Archived),
            _ => Err(ValueError::InvalidValue(value.to_string())),
        }
    }
}

/// A patch revision.
#[derive(Debug, Clone, Serialize)]
pub struct Revision<T = ()> {
    /// Unique revision ID. This is useful in case of conflicts, eg.
    /// a user published a revision from two devices by mistake.
    pub id: RevisionId,
    /// Base branch commit (merge base).
    pub base: git::Oid,
    /// Reference to the Git object containing the code (revision head).
    pub oid: git::Oid,
    /// "Cover letter" for this changeset.
    pub comment: Comment,
    /// Discussion around this revision.
    pub discussion: Discussion,
    /// Reviews (one per user) of the changes.
    pub reviews: HashMap<NodeId, Review>,
    /// Merges of this revision into other repositories.
    pub merges: Vec<Merge>,
    /// Code changeset for this revision.
    pub changeset: T,
    /// When this revision was created.
    pub timestamp: Timestamp,
}

impl Revision {
    pub fn new(
        author: Author,
        base: git::Oid,
        oid: git::Oid,
        comment: String,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            base,
            oid,
            comment: Comment::new(author, comment, timestamp),
            discussion: Discussion::default(),
            reviews: HashMap::default(),
            merges: Vec::default(),
            changeset: (),
            timestamp,
        }
    }

    pub fn description(&self) -> &str {
        &self.comment.body
    }

    pub fn author(&self) -> &Author {
        &self.comment.author
    }

    /// Put this object into an automerge document.
    fn put<'a>(
        &self,
        mut tx: impl AsMut<automerge::transaction::Transaction<'a>>,
        id: &automerge::ObjId,
    ) -> Result<(), AutomergeError> {
        assert!(
            self.merges.is_empty(),
            "Cannot put revision with non-empty merges"
        );
        assert!(
            self.reviews.is_empty(),
            "Cannot put revision with non-empty reviews"
        );
        assert!(
            self.discussion.is_empty(),
            "Cannot put revision with non-empty discussion"
        );
        let tx = tx.as_mut();

        tx.put(id, "id", self.id.to_string())?;
        tx.put(id, "oid", self.oid.to_string())?;
        tx.put(id, "base", self.base.to_string())?;

        self.comment.put(tx, id)?;

        tx.put_object(id, "discussion", ObjType::List)?;
        tx.put_object(id, "reviews", ObjType::Map)?;
        tx.put_object(id, "merges", ObjType::List)?;
        tx.put(id, "timestamp", self.timestamp)?;

        Ok(())
    }
}

/// A merged patch revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Merge {
    /// Owner of repository that this patch was merged into.
    pub node: NodeId,
    /// Base branch commit that contains the revision.
    pub commit: git::Oid,
    /// When this merged was performed.
    pub timestamp: Timestamp,
}

/// A patch review verdict.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

impl From<Verdict> for ScalarValue {
    fn from(verdict: Verdict) -> Self {
        #[allow(clippy::unwrap_used)]
        let s = serde_json::to_string(&verdict).unwrap(); // Cannot fail.
        ScalarValue::from(s)
    }
}

impl<'a> FromValue<'a> for Verdict {
    fn from_value(value: Value) -> Result<Self, ValueError> {
        let verdict = value.to_str().ok_or(ValueError::InvalidType)?;
        serde_json::from_str(verdict).map_err(|e| ValueError::Other(Arc::new(e)))
    }
}

/// Code location, used for attaching comments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeLocation {
    /// Line number commented on.
    pub lines: RangeInclusive<usize>,
    /// Commit commented on.
    pub commit: git::Oid,
    /// File being commented on.
    pub blob: git::Oid,
}

/// Comment on code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeComment {
    /// Code location of the comment.
    location: CodeLocation,
    /// Comment.
    comment: Comment,
}

/// A patch review on a revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// Review author.
    pub author: Author,
    /// Review verdict.
    pub verdict: Option<Verdict>,
    /// Review general comment.
    pub comment: Comment<Replies>,
    /// Review inline code comments.
    pub inline: Vec<CodeComment>,
    /// Review timestamp.
    pub timestamp: Timestamp,
}

impl Review {
    pub fn new(
        author: Author,
        verdict: Option<Verdict>,
        comment: impl Into<String>,
        inline: Vec<CodeComment>,
        timestamp: Timestamp,
    ) -> Self {
        let comment = Comment::new(author.clone(), comment.into(), timestamp);

        Self {
            author,
            verdict,
            comment,
            inline,
            timestamp,
        }
    }

    /// Put this object into an automerge document.
    fn put<'a>(
        &self,
        mut tx: impl AsMut<automerge::transaction::Transaction<'a>>,
        id: &automerge::ObjId,
    ) -> Result<(), AutomergeError> {
        assert!(
            self.inline.is_empty(),
            "Cannot put review with non-empty inline comments"
        );
        let tx = tx.as_mut();

        tx.put(id, "author", &self.author)?;
        tx.put(
            id,
            "verdict",
            if let Some(v) = self.verdict {
                v.into()
            } else {
                ScalarValue::Null
            },
        )?;

        self.comment.put(tx, id)?;

        tx.put_object(id, "inline", ObjType::List)?;
        tx.put(id, "timestamp", self.timestamp)?;

        Ok(())
    }
}

mod lookup {
    use super::*;

    pub fn revision(
        doc: Document,
        revision_id: &automerge::ObjId,
    ) -> Result<Revision, DocumentError> {
        let comment_id = doc.get_id(revision_id, "comment")?;
        let reviews_id = doc.get_id(revision_id, "reviews")?;
        let id = doc.get(revision_id, "id")?;
        let base = doc.get(revision_id, "base")?;
        let oid = doc.get(revision_id, "oid")?;
        let timestamp = doc.get(revision_id, "timestamp")?;
        let merges: Vec<Merge> = doc.list(revision_id, "merges", self::merge)?;

        // Discussion.
        let comment = shared::lookup::comment(doc, &comment_id)?;
        let discussion: Discussion = doc.list(revision_id, "discussion", shared::lookup::thread)?;

        // Reviews.
        let mut reviews: HashMap<NodeId, Review> = HashMap::new();
        for key in (*doc).keys(&reviews_id) {
            let review_id = doc.get_id(&reviews_id, key)?;
            let review = self::review(doc, &review_id)?;

            reviews.insert(*review.author.id(), review);
        }

        Ok(Revision {
            id,
            base,
            oid,
            comment,
            discussion,
            reviews,
            merges,
            changeset: (),
            timestamp,
        })
    }

    pub fn merge(doc: Document, obj_id: &automerge::ObjId) -> Result<Merge, DocumentError> {
        let node = doc.get(obj_id, "peer")?;
        let commit = doc.get(obj_id, "commit")?;
        let timestamp = doc.get(obj_id, "timestamp")?;

        Ok(Merge {
            node,
            commit,
            timestamp,
        })
    }

    pub fn review(doc: Document, obj_id: &automerge::ObjId) -> Result<Review, DocumentError> {
        let author = doc.get(obj_id, "author")?;
        let verdict = doc.get(obj_id, "verdict")?;
        let timestamp = doc.get(obj_id, "timestamp")?;
        let comment = doc.lookup(obj_id, "comment", shared::lookup::thread)?;
        let inline = vec![];

        Ok(Review {
            author: Author::new(author),
            comment,
            verdict,
            inline,
            timestamp,
        })
    }
}

/// Patch events.
mod events {
    use super::*;
    use automerge::{
        transaction::{CommitOptions, Transactable},
        ObjId,
    };

    pub fn create(
        author: &Author,
        title: &str,
        revision: &Revision,
        target: MergeTarget,
        timestamp: Timestamp,
        labels: &[Label],
    ) -> Result<Contents, TransactionError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(TransactionError::InvalidValue("title"));
        }

        let mut doc = Automerge::new();
        let _patch = doc
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Create patch".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let patch_id = tx.put_object(ObjId::Root, "patch", ObjType::Map)?;

                    tx.put(&patch_id, "title", title)?;
                    tx.put(&patch_id, "author", author)?;
                    tx.put(&patch_id, "state", State::Proposed)?;
                    tx.put(&patch_id, "target", target)?;
                    tx.put(&patch_id, "timestamp", timestamp)?;

                    let labels_id = tx.put_object(&patch_id, "labels", ObjType::Map)?;
                    for label in labels {
                        tx.put(&labels_id, label.name().trim(), true)?;
                    }

                    let revisions_id = tx.put_object(&patch_id, "revisions", ObjType::List)?;
                    let revision_id = tx.insert_object(&revisions_id, 0, ObjType::Map)?;

                    revision.put(tx, &revision_id)?;

                    Ok(patch_id)
                },
            )
            .map_err(|failure| failure.error)?
            .result;

        Ok(Contents::Automerge(doc.save_incremental()))
    }

    pub fn comment(
        patch: &mut Automerge,
        revision_ix: RevisionIx,
        author: &Author,
        body: &str,
        timestamp: Timestamp,
    ) -> Result<Contents, TransactionError> {
        let _comment = patch
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Add comment".to_owned()),
                |t| {
                    let mut tx = Transaction::new(t);
                    let (_, obj_id) = tx.get(ObjId::Root, "patch")?;
                    let (_, revisions_id) = tx.get(&obj_id, "revisions")?;
                    let (_, revision_id) = tx.get(&revisions_id, revision_ix)?;
                    let (_, discussion_id) = tx.get(&revision_id, "discussion")?;

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
        let change = patch.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(Contents::Automerge(change))
    }

    pub fn update(
        patch: &mut Automerge,
        revision: Revision,
    ) -> Result<(RevisionIx, Contents), TransactionError> {
        let revision_ix = patch
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Merge revision".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "patch")?;
                    let (_, revisions_id) = tx.get(&obj_id, "revisions")?;

                    let ix = tx.length(&revisions_id);
                    let revision_id = tx.insert_object(&revisions_id, ix, ObjType::Map)?;

                    revision.put(tx, &revision_id)?;

                    Ok(ix)
                },
            )
            .map_err(|failure| failure.error)?
            .result;

        #[allow(clippy::unwrap_used)]
        let change = patch.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok((revision_ix, Contents::Automerge(change)))
    }

    pub fn reply(
        patch: &mut Automerge,
        revision_ix: RevisionIx,
        comment_id: CommentId,
        author: &Author,
        body: &str,
        timestamp: Timestamp,
    ) -> Result<Contents, TransactionError> {
        patch
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Reply".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "patch")?;
                    let (_, revisions_id) = tx.get(&obj_id, "revisions")?;
                    let (_, revision_id) = tx.get(&revisions_id, revision_ix)?;
                    let (_, discussion_id) = tx.get(&revision_id, "discussion")?;
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
        let change = patch.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(Contents::Automerge(change))
    }

    pub fn review(
        patch: &mut Automerge,
        revision_ix: RevisionIx,
        review: Review,
    ) -> Result<((), Contents), TransactionError> {
        patch
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Review patch".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "patch")?;
                    let (_, revisions_id) = tx.get(&obj_id, "revisions")?;
                    let (_, revision_id) = tx.get(&revisions_id, revision_ix)?;
                    let (_, reviews_id) = tx.get(&revision_id, "reviews")?;

                    let review_id =
                        tx.put_object(&reviews_id, review.author.id.to_human(), ObjType::Map)?;

                    review.put(tx, &review_id)?;

                    Ok(())
                },
            )
            .map_err(|failure| failure.error)?;

        #[allow(clippy::unwrap_used)]
        let change = patch.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(((), Contents::Automerge(change)))
    }

    pub fn merge(
        patch: &mut Automerge,
        revision_ix: RevisionIx,
        merge: &Merge,
    ) -> Result<Contents, TransactionError> {
        patch
            .transact_with::<_, _, TransactionError, _, ()>(
                |_| CommitOptions::default().with_message("Merge revision".to_owned()),
                |tx| {
                    let mut tx = Transaction::new(tx);
                    let (_, obj_id) = tx.get(ObjId::Root, "patch")?;
                    let (_, revisions_id) = tx.get(&obj_id, "revisions")?;
                    let (_, revision_id) = tx.get(&revisions_id, revision_ix)?;
                    let (_, merges_id) = tx.get(&revision_id, "merges")?;

                    let length = tx.length(&merges_id);
                    let merge_id = tx.insert_object(&merges_id, length, ObjType::Map)?;

                    tx.put(&merge_id, "peer", merge.node.to_string())?;
                    tx.put(&merge_id, "commit", merge.commit.to_string())?;
                    tx.put(&merge_id, "timestamp", merge.timestamp)?;

                    Ok(())
                },
            )
            .map_err(|failure| failure.error)?;

        #[allow(clippy::unwrap_used)]
        let change = patch.get_last_local_change().unwrap().raw_bytes().to_vec();

        Ok(Contents::Automerge(change))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test;

    #[test]
    fn test_patch_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let patches = store.patches();
        let author = *signer.public_key();
        let timestamp = Timestamp::now();
        let target = MergeTarget::Delegates;
        let oid = git::Oid::from(git2::Oid::zero());
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let patch_id = patches
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
        let patch = patches.get(&patch_id).unwrap().unwrap();

        assert_eq!(&patch.title, "My first patch");
        assert_eq!(patch.author.id(), &author);
        assert_eq!(patch.state, State::Proposed);
        assert!(patch.timestamp >= timestamp);

        let revision = patch.revisions.head;

        assert_eq!(revision.author(), &store.author());
        assert_eq!(revision.comment.body, "Blah blah blah.");
        assert_eq!(revision.discussion.len(), 0);
        assert_eq!(revision.oid, oid);
        assert_eq!(revision.base, base);
        assert!(revision.reviews.is_empty());
        assert!(revision.merges.is_empty());
    }

    #[test]
    fn test_patch_merge() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let patches = store.patches();
        let target = MergeTarget::Delegates;
        let oid = git::Oid::from(git2::Oid::zero());
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let patch_id = patches
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

        let _merge = patches.merge(&patch_id, 0, base, &signer).unwrap();
        let patch = patches.get(&patch_id).unwrap().unwrap();
        let merges = patch.revisions.head.merges;

        assert_eq!(merges.len(), 1);
        assert_eq!(merges[0].node, *signer.public_key());
        assert_eq!(merges[0].commit, base);
    }

    #[test]
    fn test_patch_review() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let patches = store.patches();
        let whoami = store.author();
        let target = MergeTarget::Delegates;
        let base = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let rev_oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let patch_id = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                target,
                base,
                rev_oid,
                &[],
                &signer,
            )
            .unwrap();

        patches
            .review(&patch_id, 0, Some(Verdict::Accept), "LGTM", vec![], &signer)
            .unwrap();
        let patch = patches.get(&patch_id).unwrap().unwrap();
        let reviews = patch.revisions.head.reviews;
        assert_eq!(reviews.len(), 1);

        let review = reviews.get(whoami.id()).unwrap();
        assert_eq!(review.author.id(), whoami.id());
        assert_eq!(review.verdict, Some(Verdict::Accept));
        assert_eq!(review.comment.body.as_str(), "LGTM");
    }

    #[test]
    fn test_patch_update() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let patches = store.patches();
        let target = MergeTarget::Delegates;
        let base = git::Oid::from_str("af08e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let rev0_oid = git::Oid::from_str("518d5069f94c03427f694bb494ac1cd7d1339380").unwrap();
        let rev1_oid = git::Oid::from_str("cb18e95ada2bb38aadd8e6cef0963ce37a87add3").unwrap();
        let patch_id = patches
            .create(
                "My first patch",
                "Blah blah blah.",
                target,
                base,
                rev0_oid,
                &[],
                &signer,
            )
            .unwrap();

        let patch = patches.get(&patch_id).unwrap().unwrap();
        assert_eq!(patch.description(), "Blah blah blah.");
        assert_eq!(patch.version(), 0);

        let revision_id = patches
            .update(&patch_id, "I've made changes.", base, rev1_oid, &signer)
            .unwrap();

        assert_eq!(revision_id, 1);

        let patch = patches.get(&patch_id).unwrap().unwrap();
        assert_eq!(patch.description(), "I've made changes.");

        assert_eq!(patch.revisions.len(), 2);
        assert_eq!(patch.version(), 1);

        let (id, revision) = patch.latest();

        assert_eq!(id, 1);
        assert_eq!(revision.oid, rev1_oid);
        assert_eq!(revision.description(), "I've made changes.");
    }
}
