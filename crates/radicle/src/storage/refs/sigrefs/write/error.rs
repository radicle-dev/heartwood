use radicle_oid::Oid;
use thiserror::Error;

use crate::git::repository::object;
use crate::git::repository::reference;

// TODO: use commit NID (and RID?) for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Write {
    #[error(transparent)]
    Head(Head),
    #[error(transparent)]
    Commit(Commit),
    #[error(transparent)]
    Reference(reference::error::write::WriteRef),
}

// TODO: use commit OID for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Commit {
    #[error(transparent)]
    Tree(Tree),
    #[error(transparent)]
    Write(object::error::write::Commit),
}

// TODO: use commit OID for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Tree {
    #[error("failed to sign references payload")]
    Sign(crypto::signature::Error),
    #[error(transparent)]
    Write(object::error::write::Tree),
}

// TODO: use commit OID for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
#[error(transparent)]
pub enum Head {
    #[error(transparent)]
    Reference(reference::error::read::RefTarget),
    #[error(transparent)]
    Commit(super::read::error::Commit),
    #[error("failed to verify commit {commit}: {source}")]
    Verify {
        commit: Oid,
        source: super::read::error::Verify,
    },
}
