use std::path::PathBuf;

use thiserror::Error;

use crate::{git::raw, git::Oid, prelude::Did};

use super::CanonicalObject;

/// Error that can occur when calculation the [`Canonical::quorum`].
#[derive(Debug, Error)]
pub enum QuorumError {
    /// Could not determine a quorum [`Oid`], due to diverging tips.
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
    #[error("could not determine target for canonical reference '{refname}', found objects of different types")]
    DifferentTypes { refname: String },
    /// Could not determine a base candidate from the given set of delegates.
    #[error("could not determine target for canonical reference '{refname}', no object with at least {threshold} vote(s) found (threshold not met)")]
    NoCandidates { refname: String, threshold: usize },
    /// An error occurred from [`git2`].
    #[error(transparent)]
    Git(#[from] git2::Error),
}

#[derive(Debug, Error)]
#[error("failed to check if {head} is an ancestor of {canonical} due to: {source}")]
pub struct GraphDescendant {
    head: Oid,
    canonical: Oid,
    source: raw::Error,
}

#[derive(Debug, Error)]
#[error("the commit {commit} for {did} is missing from the repository {repo:?}")]
pub struct MissingObject {
    repo: PathBuf,
    did: Did,
    commit: Oid,
    source: raw::Error,
}

#[derive(Debug, Error)]
#[error("could not determine whether the commit {commit} for {did} is part of the repository {repo:?} due to: {source}")]
pub struct InvalidObject {
    repo: PathBuf,
    did: Did,
    commit: Oid,
    source: raw::Error,
}

#[derive(Debug, Error)]
#[error("the object {oid} for {did} in the repository {repo:?} is of unexpected type {kind:?}")]
pub struct InvalidObjectType {
    repo: PathBuf,
    did: Did,
    oid: Oid,
    kind: Option<git2::ObjectType>,
}

#[derive(Debug, Error)]
#[error("the object {oid} in the repository {repo:?} is of unexpected type {found} and was expected to be {expected}")]
pub struct MismatchedObject {
    repo: PathBuf,
    oid: Oid,
    found: CanonicalObject,
    expected: CanonicalObject,
}

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error(transparent)]
    InvalidObjectType(#[from] InvalidObjectType),
    #[error(transparent)]
    MissingObject(#[from] MissingObject),
    #[error("failed to find object {oid} due to: {source}")]
    FindObject { oid: Oid, source: git2::Error },
    #[error("failed to find reference {name} due to: {source}")]
    FindReference { name: String, source: git2::Error },
}

impl CanonicalError {
    pub(super) fn invalid_object_type(
        repo: PathBuf,
        did: Did,
        oid: Oid,
        kind: Option<git2::ObjectType>,
    ) -> Self {
        InvalidObjectType {
            repo,
            did,
            oid,
            kind,
        }
        .into()
    }

    pub(super) fn missing_object(repo: PathBuf, did: Did, oid: Oid, err: git2::Error) -> Self {
        MissingObject {
            repo,
            did,
            commit: oid,
            source: err,
        }
        .into()
    }

    pub(super) fn find_object(oid: Oid, err: git2::Error) -> Self {
        Self::FindObject { oid, source: err }
    }

    pub(crate) fn find_reference(name: &str, e: git2::Error) -> CanonicalError {
        Self::FindReference {
            name: name.to_string(),
            source: e,
        }
    }
}

#[derive(Debug, Error)]
pub enum FindObjectError {
    #[error(transparent)]
    InvalidObjectType(#[from] InvalidObjectType),
    #[error(transparent)]
    MissingObject(#[from] MissingObject),
    #[error("failed to find object {oid} due to: {source}")]
    FindObject { oid: Oid, source: git2::Error },
}

impl FindObjectError {
    pub(super) fn find_object(oid: Oid, err: git2::Error) -> Self {
        Self::FindObject { oid, source: err }
    }

    pub(super) fn missing_object(repo: PathBuf, did: Did, oid: Oid, err: git2::Error) -> Self {
        MissingObject {
            repo,
            did,
            commit: oid,
            source: err,
        }
        .into()
    }

    pub(super) fn invalid_object_type(
        repo: PathBuf,
        did: Did,
        oid: Oid,
        kind: Option<git2::ObjectType>,
    ) -> Self {
        InvalidObjectType {
            repo,
            did,
            oid,
            kind,
        }
        .into()
    }
}

#[derive(Debug, Error)]
pub enum ConvergesError {
    #[error(transparent)]
    GraphDescendant(#[from] GraphDescendant),
    #[error(transparent)]
    MismatchedObject(#[from] MismatchedObject),
    #[error(transparent)]
    FindObjectError(#[from] FindObjectError),
}

impl ConvergesError {
    pub(super) fn graph_descendant(head: Oid, canonical: Oid, source: raw::Error) -> Self {
        Self::GraphDescendant(GraphDescendant {
            head,
            canonical,
            source,
        })
    }

    pub(super) fn mismatched_object(
        repo: PathBuf,
        oid: Oid,
        found: CanonicalObject,
        expected: CanonicalObject,
    ) -> Self {
        Self::MismatchedObject(MismatchedObject {
            repo,
            oid,
            found,
            expected,
        })
    }
}
