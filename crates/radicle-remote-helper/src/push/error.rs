use radicle::git;
use radicle::git::canonical;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CanonicalUnrecoverable {
    #[error(transparent)]
    GraphDescendant(#[from] GraphDescendant),
    #[error(transparent)]
    Converges(#[from] canonical::error::ConvergesError),
    #[error(transparent)]
    MergeBase(#[from] canonical::error::MergeBaseError),
    #[error(transparent)]
    FindObjects(#[from] canonical::error::FindObjectsError),
    #[error(transparent)]
    HeadsDiverge(#[from] HeadsDiverge),
    #[error("failure while computing canonical reference: {source}")]
    Git { source: git::raw::Error },
}

#[derive(Debug, Error)]
pub enum Canonical {
    #[error(transparent)]
    GraphDescendant(GraphDescendant),
    #[error(transparent)]
    HeadsDiverge(HeadsDiverge),
    #[error(transparent)]
    Quorum(#[from] canonical::error::QuorumError),
}

impl Canonical {
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
}

#[derive(Debug, Error)]
#[error("failed to check if {head} is an ancestor of {canonical} due to: {source}")]
pub struct GraphDescendant {
    head: git::Oid,
    canonical: git::Oid,
    source: git::raw::Error,
}

#[derive(Debug, Error)]
/// Head being pushed diverges from canonical head.
#[error("refusing to update canonical reference to commit that is not a descendant of current canonical head")]
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
