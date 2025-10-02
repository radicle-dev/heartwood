use std::{
    convert::TryFrom,
    path::{Path, PathBuf},
};

use crate::{Commit, Error, Repository, ToCommit};

/// An iterator that produces the history of commits for a given `head`.
///
/// The lifetime of this struct is attached to the underlying [`Repository`].
pub struct History<'a> {
    repo: &'a Repository,
    head: Commit,
    revwalk: git2::Revwalk<'a>,
    filter_by: Option<FilterBy>,
}

/// Internal implementation, subject to refactoring.
enum FilterBy {
    File { path: PathBuf },
}

impl<'a> History<'a> {
    /// Creates a new history starting from `head`, in `repo`.
    pub(crate) fn new<C: ToCommit>(repo: &'a Repository, head: C) -> Result<Self, Error> {
        let head = head
            .to_commit(repo)
            .map_err(|err| Error::ToCommit(err.into()))?;
        let mut revwalk = repo.revwalk()?;
        revwalk.push(head.id.into())?;
        let history = Self {
            repo,
            head,
            revwalk,
            filter_by: None,
        };
        Ok(history)
    }

    /// Returns the first commit (i.e. the head) in the history.
    pub fn head(&self) -> &Commit {
        &self.head
    }

    /// Returns a modified `History` filtered by `path`.
    ///
    /// Note that it is possible that a filtered History becomes empty,
    /// even though calling `.head()` still returns the original head.
    pub fn by_path<P>(mut self, path: &P) -> Self
    where
        P: AsRef<Path>,
    {
        self.filter_by = Some(FilterBy::File {
            path: path.as_ref().to_path_buf(),
        });
        self
    }
}

impl Iterator for History<'_> {
    type Item = Result<Commit, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Loop through the commits with the optional filtering.
        while let Some(oid) = self.revwalk.next() {
            let found = oid
                .map_err(Error::Git)
                .and_then(|oid| {
                    let commit = self.repo.find_commit(oid.into())?;

                    // Handles the optional filter_by.
                    if let Some(FilterBy::File { path }) = &self.filter_by {
                        // Only check the commit diff if the path is not empty.
                        if !path.as_os_str().is_empty() {
                            let path_opt = self.repo.diff_commit_and_parents(path, &commit)?;
                            if path_opt.is_none() {
                                return Ok(None); // Filter out this commit.
                            }
                        }
                    }

                    let commit = Commit::try_from(commit)?;
                    Ok(Some(commit))
                })
                .transpose();
            if found.is_some() {
                return found;
            }
        }
        None
    }
}

impl std::fmt::Debug for History<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "History of {}", self.head.id)
    }
}
