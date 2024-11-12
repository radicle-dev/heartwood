use std::marker::PhantomData;

use serde::Deserialize;

use crate::cob::{Op, TypeName};
use crate::git::{self, Oid, PatternString};

use super::error;
use super::CobRange;

/// A `Walk` specifies a range to construct a [`WalkIter`].
#[derive(Clone, Debug)]
pub(super) struct Walk {
    from: Oid,
    until: Until,
}

/// Specify the end of a range by either providing an [`Oid`] tip, or a
/// reference glob via a [`PatternString`].
#[derive(Clone, Debug)]
pub enum Until {
    Tip(Oid),
    Glob(PatternString),
}

impl From<Oid> for Until {
    fn from(tip: Oid) -> Self {
        Self::Tip(tip)
    }
}

impl From<PatternString> for Until {
    fn from(glob: PatternString) -> Self {
        Self::Glob(glob)
    }
}

/// A revwalk over a set of commits, including the commit that is being walked
/// from.
pub(super) struct WalkIter<'a> {
    /// Git repository for looking up the commit object during the revwalk.
    repo: &'a git2::Repository,
    /// The root commit that is being walked from.
    ///
    /// N.b. This is required since ranges are non-inclusive in Git, and if the
    /// `^` notation is used with a root commit, then it will result in an
    /// error.
    from: Option<Oid>,
    /// The revwalk that is being iterated over.
    inner: git2::Revwalk<'a>,
}

impl From<CobRange> for Walk {
    fn from(history: CobRange) -> Self {
        Self::new(history.root, history.until)
    }
}

impl Walk {
    /// Construct a new `Walk`, `from` the given commit, `until` the end of a
    /// given range.
    pub(super) fn new(from: Oid, until: Until) -> Self {
        Self { from, until }
    }

    /// Change the `Oid` that the walk starts from.
    pub(super) fn since(mut self, from: Oid) -> Self {
        self.from = from;
        self
    }

    /// Change the `Until` that the walk finishes on.
    pub(super) fn until(mut self, until: impl Into<Until>) -> Self {
        self.until = until.into();
        self
    }

    /// Get the iterator for the walk.
    pub(super) fn iter(self, repo: &git2::Repository) -> Result<WalkIter<'_>, git2::Error> {
        let mut walk = repo.revwalk()?;
        // N.b. ensure that we start from the `self.from` commit.
        walk.set_sorting(git2::Sort::TOPOLOGICAL.union(git2::Sort::REVERSE))?;
        match self.until {
            Until::Tip(tip) => walk.push_range(&format!("{}..{}", self.from, tip))?,
            Until::Glob(glob) => {
                walk.push(*self.from)?;
                walk.push_glob(glob.as_str())?
            }
        }

        Ok(WalkIter {
            repo,
            from: Some(self.from),
            inner: walk,
        })
    }
}

impl<'a> Iterator for WalkIter<'a> {
    type Item = Result<git2::Commit<'a>, git2::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // N.b. ensure that we start using the `from` commit and use the revwalk
        // after that.
        if let Some(from) = self.from.take() {
            return Some(self.repo.find_commit(*from));
        }
        let oid = self.inner.next()?;
        Some(oid.and_then(|oid| self.repo.find_commit(oid)))
    }
}

/// Iterate over all operations for a given range of commits.
pub struct OpsIter<'a, A> {
    /// The [`WalkIter`] provides each commit that it is being walked over for a
    /// given range.
    walk: WalkIter<'a>,
    /// The walk can iterate over other COBs, e.g. an Identity COB, so this is
    /// used to filter for the correct type.
    typename: TypeName,
    /// Marker for the type of action that is associated with the Op
    action: PhantomData<A>,
}

impl<'a, A> Iterator for OpsIter<'a, A>
where
    A: for<'de> Deserialize<'de>,
{
    type Item = Result<Op<A>, error::Ops>;

    fn next(&mut self) -> Option<Self::Item> {
        let commit = self.walk.next()?;
        match commit {
            Ok(commit) => {
                let entry = git::Oid::from(commit.id());
                // N.b. mark this commit as seen, so that it is not walked again
                self.walk.inner.hide(commit.id()).ok();
                // Skip any Op that do not match the manifest
                self.load(entry).transpose().or_else(|| self.next())
            }
            // Something was wrong with the commit
            Err(err) => Some(Err(error::Ops::Commit { err })),
        }
    }
}

impl<'a, A> OpsIter<'a, A> {
    pub(super) fn new(walk: WalkIter<'a>, typename: TypeName) -> Self {
        Self {
            walk,
            typename,
            action: PhantomData,
        }
    }

    /// Load the `Op` for the given `entry`, ensuring that manifest matches with
    /// the expected manifest.
    fn load(&self, entry: git::Oid) -> Result<Option<Op<A>>, error::Ops>
    where
        A: for<'de> Deserialize<'de>,
    {
        let manifest = Op::<A>::manifest_of(self.walk.repo, &entry)
            .map_err(|err| error::Ops::Manifest { err })?;
        if manifest.type_name == self.typename {
            let op = Op::load(self.walk.repo, entry).map_err(|err| error::Ops::Load { err })?;
            Ok(Some(op))
        } else {
            Ok(None)
        }
    }
}
