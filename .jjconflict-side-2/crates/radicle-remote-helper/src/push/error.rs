use radicle::git;
use radicle::git::canonical;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CanonicalUnrecoverable {
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
}

#[derive(Debug, Error)]
#[error("failed to check if {head} is an ancestor of {canonical} due to: {source}")]
pub(crate) struct GraphDescendant {
    head: git::Oid,
    canonical: git::Oid,
    source: git::raw::Error,
}

#[derive(Debug, Error)]
/// Head being pushed diverges from canonical head.
#[error(
    "refusing to update canonical reference to commit that is not a descendant of current canonical head"
)]
pub(crate) struct HeadsDiverge {
    head: git::Oid,
    canonical: git::Oid,
}

#[derive(Debug, Error)]
pub(crate) enum PushAction {
    #[error("invalid reference {refname}, expected qualified reference starting with `refs/`")]
    InvalidRef { refname: git::fmt::RefString },
    #[error("found refs/heads/patches/{suffix} where {suffix} was an invalid Patch ID: {source}")]
    InvalidPatchId {
        suffix: String,
        source: radicle::git::ParseOidError,
    },
}
