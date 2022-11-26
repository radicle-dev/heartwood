use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::RangeInclusive;
use std::str::FromStr;

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::cob::common::*;
use crate::cob::{ObjectId, Timestamp, TypeName};
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
    pub labels: HashSet<Tag>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Draft,
    Proposed,
    Archived,
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
}
