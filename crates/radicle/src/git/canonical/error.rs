use thiserror::Error;

use crate::git::Oid;

use super::{effects, ObjectType};
pub use effects::{FindObjectsError, MergeBaseError};

#[derive(Debug, Error)]
pub enum QuorumError {
    #[error("could not determine target for canonical reference '{refname}', found objects of different types")]
    DifferentTypes { refname: String },
    #[error(transparent)]
    Convergence(#[from] ConvergesError),
    #[error(transparent)]
    MergeBase(#[from] MergeBaseError),
    #[error("could not determine target for canonical reference '{refname}', no object with at least {threshold} vote(s) found (threshold not met)")]
    NoCandidates { refname: String, threshold: usize },
    #[error("could not determine target commit for canonical reference '{refname}', found diverging commits {longest} and {head}, with base commit {base} and threshold {threshold}")]
    DivergingCommits {
        refname: String,
        threshold: usize,
        base: Oid,
        longest: Oid,
        head: Oid,
    },
    #[error("could not determine target tag for canonical reference '{refname}', found multiple candidates with threshold {threshold}")]
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
    GraphDescendant(#[from] effects::GraphDescendant),
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
