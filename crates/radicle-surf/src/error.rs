//! Definition for a crate level error type, which wraps up module level
//! error types transparently.

use crate::{commit, diff, fs, glob, namespace, refs, repo};
use thiserror::Error;

/// The crate level error type that wraps up module level error types.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Branches(#[from] refs::error::Branch),
    #[error(transparent)]
    Categories(#[from] refs::error::Category),
    #[error(transparent)]
    Commit(#[from] commit::Error),
    #[error(transparent)]
    Diff(#[from] diff::git::error::Diff),
    #[error(transparent)]
    Directory(#[from] fs::error::Directory),
    #[error(transparent)]
    File(#[from] fs::error::File),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Glob(#[from] glob::Error),
    #[error(transparent)]
    Namespace(#[from] namespace::Error),
    #[error(transparent)]
    RefFormat(#[from] git_ext::ref_format::Error),
    #[error(transparent)]
    Revision(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error(transparent)]
    ToCommit(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error(transparent)]
    Tags(#[from] refs::error::Tag),
    #[error(transparent)]
    Repo(#[from] repo::error::Repo),
}
