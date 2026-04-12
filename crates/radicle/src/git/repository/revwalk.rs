//! Git commit graph walk trait.
//!
//! [`Revwalk`] provides commit iterators, given a [`RevwalkPlan`].

pub mod error;

use radicle_oid::Oid;

use super::types::Commit;

/// The sort order for a [`RevwalkPlan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// Chronological order (newest first, by commit time).
    Chronological {
        /// Setting `reverse` to `true` will sort in reverse-chronological
        /// order (oldest first, by commit time).
        reverse: bool,
    },
    /// Topological order (parents before children).
    Topological {
        /// Setting `reverse` to `true` will sort in reverse-topological order
        /// (children before parents).
        reverse: bool,
    },
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::Chronological { reverse: false }
    }
}

/// A plan for walking the commit graph.
///
/// Accumulates configuration (start points, hidden commits, sort order)
/// and is finalised by passing it to an [`Revwalk`] implementation.
#[derive(Clone, Debug, Default)]
pub struct RevwalkPlan {
    start: Vec<Oid>,
    hide: Vec<Oid>,
    range: Option<(Oid, Oid)>,
    sort: SortOrder,
}

impl RevwalkPlan {
    /// Create a default walk, that walks all commits in chronological order.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a starting point for the walk.
    pub fn push(mut self, oid: Oid) -> Self {
        self.start.push(oid);
        self
    }

    /// Exclude commits reachable from this [`Oid`].
    pub fn hide(mut self, oid: Oid) -> Self {
        self.hide.push(oid);
        self
    }

    /// Walk only commits in the range `from..to` (commits reachable from
    /// `to` but not from `from`).
    pub fn range(mut self, from: Oid, to: Oid) -> Self {
        self.range = Some((from, to));
        self
    }

    /// Set the sort order for the walk.
    pub fn sort(mut self, order: SortOrder) -> Self {
        self.sort = order;
        self
    }

    /// The starting points for the walk.
    pub fn starts(&self) -> &[Oid] {
        &self.start
    }

    /// The commits to hide (exclude reachable commits).
    pub fn hidden(&self) -> &[Oid] {
        &self.hide
    }

    /// The range, if set.
    pub fn range_bounds(&self) -> Option<(Oid, Oid)> {
        self.range
    }

    /// The sort order.
    pub fn sort_order(&self) -> SortOrder {
        self.sort
    }
}

/// Git commit graph walks.
pub trait Revwalk {
    /// Iterator of commit [`Oid`]s.
    type RevwalkOids<'a>: Iterator<Item = Result<Oid, error::Oids>> + 'a
    where
        Self: 'a;

    /// Iterator of [`Commit`]s.
    type RevwalkCommits<'a>: Iterator<Item = Result<Commit, error::Commit>> + 'a
    where
        Self: 'a;

    /// Execute a revwalk plan, returning an iterator of commit [`Oid`]s.
    ///
    /// The returned iterator yields [`Oids`] for per-step failures:
    /// - [`Oids::Backend`]: An unexpected error during iteration.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error when initialising the walk.
    ///
    /// [`Backend`]: error::Init::Backend
    /// [`Oids`]: error::Oids
    /// [`Oids::Backend`]: error::Oids::Backend
    fn revwalk_oids<'a>(&'a self, plan: &RevwalkPlan)
    -> Result<Self::RevwalkOids<'a>, error::Init>;

    /// Execute a revwalk plan, returning an iterator of full [`Commit`] data.
    ///
    /// More expensive than [`Self::revwalk_oids`] since each commit is fully
    /// parsed during iteration.
    ///
    /// The returned iterator yields [`error::Commit`] for per-step failures:
    /// - [`error::Commit::Parse`]: A commit's raw bytes could not be parsed.
    /// - [`error::Commit::Backend`]: An unexpected error during iteration.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error when initialising the walk.
    ///
    /// [`Backend`]: error::Init::Backend
    fn revwalk_commits<'a>(
        &'a self,
        plan: &RevwalkPlan,
    ) -> Result<Self::RevwalkCommits<'a>, error::Init>;
}
