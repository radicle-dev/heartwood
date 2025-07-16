use std::path::PathBuf;

use radicle::git;
use radicle::git::canonical;
use radicle::prelude::Did;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CanonicalUnrecoverable {
    #[error(transparent)]
    GraphDescendant(#[from] GraphDescendant),
    #[error(transparent)]
    Converges(#[from] Converges),
    #[error(transparent)]
    HeadsDiverge(#[from] HeadsDiverge),
    #[error(transparent)]
    MissingCommit(#[from] MissingCommit),
    #[error(transparent)]
    InvalidCommit(#[from] InvalidCommit),
    #[error("failure while computing canonical reference: {source}")]
    Git { source: git::raw::Error },
}

#[derive(Debug, Error)]
pub enum Canonical {
    #[error(transparent)]
    GraphDescendant(GraphDescendant),
    #[error(transparent)]
    Converges(Converges),
    #[error(transparent)]
    HeadsDiverge(HeadsDiverge),
    #[error(transparent)]
    Quorum(#[from] canonical::QuorumError),
    #[error(transparent)]
    MissingCommit(MissingCommit),
    #[error(transparent)]
    InvalidCommit(InvalidCommit),
}

impl Canonical {
    pub fn converges(head: git::Oid, source: git::raw::Error) -> Self {
        Self::Converges(Converges { head, source })
    }

    pub fn graph_descendant(head: git::Oid, canonical: git::Oid, source: git::raw::Error) -> Self {
        Self::GraphDescendant(GraphDescendant {
            head,
            canonical,
            source,
        })
    }

    pub fn heads_diverge(head: git::Oid, canonical: git::Oid) -> Self {
        Self::HeadsDiverge(HeadsDiverge { head, canonical })
    }

    pub fn missing_commit(
        repo: PathBuf,
        did: Did,
        commit: git::Oid,
        source: git::raw::Error,
    ) -> Self {
        Self::MissingCommit(MissingCommit {
            repo,
            did,
            commit,
            source,
        })
    }

    pub fn invalid_commit(
        repo: PathBuf,
        did: Did,
        commit: git::Oid,
        source: git::raw::Error,
    ) -> Self {
        Self::InvalidCommit(InvalidCommit {
            repo,
            did,
            commit,
            source,
        })
    }
}

#[derive(Debug, Error)]
#[error("the commit {commit} for {did} is missing from the repository {repo:?}")]
pub struct MissingCommit {
    repo: PathBuf,
    did: Did,
    commit: git::Oid,
    source: git::raw::Error,
}

#[derive(Debug, Error)]
#[error("could not determine whether the commit {commit} for {did} is part of the repository {repo:?} due to: {source}")]
pub struct InvalidCommit {
    repo: PathBuf,
    did: Did,
    commit: git::Oid,
    source: git::raw::Error,
}

#[derive(Debug, Error)]
#[error("failed to check if {head} is an ancestor of {canonical} due to: {source}")]
pub struct GraphDescendant {
    head: git::Oid,
    canonical: git::Oid,
    source: git::raw::Error,
}

#[derive(Debug, Error)]
#[error("failed to see if {head} converges with other commits due to: {source}")]
pub struct Converges {
    head: git::Oid,
    source: git::raw::Error,
}

#[derive(Debug, Error)]
/// Head being pushed diverges from canonical head.
#[error("refusing to update branch to commit that is not a descendant of canonical head")]
pub struct HeadsDiverge {
    head: git::Oid,
    canonical: git::Oid,
}

#[derive(Debug, Error)]
pub enum PushAction {
    #[error("invalid reference {refname}, expected qualified reference starting with `refs/`")]
    InvalidRef { refname: git::RefString },
    #[error("found refs/heads/patches/{suffix} where {suffix} was an invalid Patch ID")]
    InvalidPatchId {
        suffix: String,
        source: git::raw::Error,
    },
}
