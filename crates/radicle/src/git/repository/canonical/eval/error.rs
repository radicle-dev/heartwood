use thiserror::Error;

use crate::git::Oid;

use crate::git::repository::ancestry;

use super::{ObjectType, objects};
pub use objects::FindObjectsError;

/// An error that occurred while computing a merge base.
///
/// Carries the two commit OIDs for context.
#[derive(Debug, thiserror::Error)]
#[error("failed to find merge base for {a} and {b}: {source}")]
pub struct MergeBaseError {
    a: Oid,
    b: Oid,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

#[derive(thiserror::Error, Debug)]
#[error("no existing merge base found for commit quorum")]
struct NoMergeBase;

#[derive(thiserror::Error, Debug)]
#[error("no common ancestor")]
struct NoCommonAncestor;

impl MergeBaseError {
    pub fn new<E>(a: Oid, b: Oid, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            a,
            b,
            source: Box::new(source),
        }
    }

    pub(super) fn no_merge_base(a: Oid, b: Oid) -> Self {
        Self::new(a, b, NoMergeBase)
    }

    pub(super) fn no_common_ancestor(a: Oid, b: Oid) -> Self {
        Self::new(a, b, NoCommonAncestor)
    }
}

#[derive(Debug, Error)]
pub enum QuorumError {
    #[error(
        "could not determine target for canonical reference '{refname}', found objects of different types"
    )]
    DifferentTypes { refname: String },
    #[error(transparent)]
    Convergence(#[from] ConvergesError),
    #[error(transparent)]
    MergeBase(#[from] MergeBaseError),
    #[error(
        "could not determine target for canonical reference '{refname}', no object with at least {threshold} vote(s) found (threshold not met)"
    )]
    NoCandidates { refname: String, threshold: usize },
    #[error(
        "could not determine target commit for canonical reference '{refname}', found diverging commits {longest} and {head}, with base commit {base} and threshold {threshold}"
    )]
    DivergingCommits {
        refname: String,
        threshold: usize,
        base: Oid,
        longest: Oid,
        head: Oid,
    },
    #[error(
        "could not determine target tag for canonical reference '{refname}', found multiple candidates with threshold {threshold}"
    )]
    DivergingTags {
        refname: String,
        threshold: usize,
        candidates: Vec<Oid>,
    },
}

#[derive(Debug, Error)]
#[error("the object {oid} is of unexpected type {found} and was expected to be {expected}")]
pub struct MismatchedObject {
    oid: Oid,
    found: ObjectType,
    expected: ObjectType,
}

#[derive(Debug, Error)]
pub enum ConvergesError {
    #[error(transparent)]
    AheadBehind(#[from] ancestry::error::AheadBehind),
    #[error(transparent)]
    MismatchedObject(#[from] MismatchedObject),
}

impl ConvergesError {
    pub(super) fn mismatched_object(oid: Oid, found: ObjectType, expected: ObjectType) -> Self {
        Self::MismatchedObject(MismatchedObject {
            oid,
            found,
            expected,
        })
    }
}
