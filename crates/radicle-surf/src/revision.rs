use std::convert::Infallible;

use git_ext::{
    ref_format::{Qualified, RefString},
    Oid,
};

use crate::{Branch, Commit, Error, Repository, Tag};

/// The signature of a commit
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signature(Vec<u8>);

impl From<git2::Buf> for Signature {
    fn from(other: git2::Buf) -> Self {
        Signature((*other).into())
    }
}

/// Supports various ways to specify a revision used in Git.
pub trait Revision {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the object id of this revision in `repo`.
    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error>;
}

impl Revision for RefString {
    type Error = git2::Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        repo.refname_to_id(self)
    }
}

impl Revision for Qualified<'_> {
    type Error = git2::Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        repo.refname_to_id(self)
    }
}

impl Revision for Oid {
    type Error = Infallible;

    fn object_id(&self, _repo: &Repository) -> Result<Oid, Self::Error> {
        Ok(*self)
    }
}

impl Revision for &str {
    type Error = git2::Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        Oid::from_str_ext(self, repo.object_format())
    }
}

impl Revision for Branch {
    type Error = Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        let refname = repo.namespaced_refname(&self.refname())?;
        Ok(repo.refname_to_id(&refname)?)
    }
}

impl Revision for Tag {
    type Error = Infallible;

    fn object_id(&self, _repo: &Repository) -> Result<Oid, Self::Error> {
        Ok(self.id())
    }
}

impl Revision for String {
    type Error = git2::Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        Oid::from_str_ext(self, repo.object_format())
    }
}

impl<R: Revision> Revision for &R {
    type Error = R::Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        (*self).object_id(repo)
    }
}

impl<R: Revision> Revision for Box<R> {
    type Error = R::Error;

    fn object_id(&self, repo: &Repository) -> Result<Oid, Self::Error> {
        self.as_ref().object_id(repo)
    }
}

/// A common trait for anything that can convert to a `Commit`.
pub trait ToCommit {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Converts to a commit in `repo`.
    fn to_commit(self, repo: &Repository) -> Result<Commit, Self::Error>;
}

impl ToCommit for Commit {
    type Error = Infallible;

    fn to_commit(self, _repo: &Repository) -> Result<Commit, Self::Error> {
        Ok(self)
    }
}

impl<R: Revision> ToCommit for R {
    type Error = Error;

    fn to_commit(self, repo: &Repository) -> Result<Commit, Self::Error> {
        let oid = repo.object_id(&self)?;
        let commit = repo.find_commit(oid)?;
        Ok(Commit::try_from(commit)?)
    }
}
