use radicle_oid::Oid;
use thiserror::Error;

use crate::storage::refs::sigrefs::git::{object, reference};

// TODO: use commit NID (and RID?) for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Write {
    #[error(transparent)]
    Head(Head),
    #[error(transparent)]
    Commit(Commit),
    #[error(transparent)]
    Reference(reference::error::WriteReference),
}

// TODO: use commit OID for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Commit {
    #[error(transparent)]
    Tree(Tree),
    #[error(transparent)]
    Write(object::error::WriteCommit),
}

// TODO: use commit OID for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Tree {
    #[error("failed to sign references payload")]
    Sign(crypto::signature::Error),
    #[error(transparent)]
    Write(object::error::WriteTree),
}

// TODO: use commit OID for traceability
#[derive(Debug, Error)]
#[non_exhaustive]
#[error(transparent)]
pub enum Head {
    #[error(transparent)]
    Reference(reference::error::FindReference),
    #[error(transparent)]
    Commit(super::read::error::Commit),
    #[error("failed to verify commit {commit}: {source}")]
    Verify {
        commit: Oid,
        source: super::read::error::Verify,
    },
}
