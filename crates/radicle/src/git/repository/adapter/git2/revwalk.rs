use radicle_oid::Oid;

use crate::git;
use crate::git::raw;

use crate::git::repository::revwalk::error;
use crate::git::repository::revwalk::{Revwalk, RevwalkPlan, SortOrder};
use crate::git::repository::types::Commit;

/// [`Revwalk::RevwalkOids`] iterator using [`raw::Revwalk`].
pub struct RevwalkOids<'a> {
    inner: raw::Revwalk<'a>,
}

impl<'a> RevwalkOids<'a> {
    pub fn hide(&mut self, oid: Oid) -> Result<(), error::Oids> {
        self.inner.hide(oid.into()).map_err(error::Oids::backend)
    }
}

impl Iterator for RevwalkOids<'_> {
    type Item = Result<Oid, error::Oids>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|r| r.map(Oid::from).map_err(error::Oids::backend))
    }
}

/// [`Revwalk::RevwalkCommits`] iterator using [`raw::Revwalk`].
pub struct RevwalkCommits<'a> {
    oids: RevwalkOids<'a>,
    backend: &'a raw::Repository,
}

impl<'a> RevwalkCommits<'a> {
    pub fn hide(&mut self, oid: Oid) -> Result<(), error::Oids> {
        self.oids.hide(oid)
    }

    fn read(backend: &raw::Repository, oid: Oid) -> Result<Commit, error::Commit> {
        let odb = backend.odb().map_err(error::Commit::backend)?;
        let obj = odb.read(oid.into()).map_err(error::Commit::backend)?;
        Commit::from_bytes(obj.data()).map_err(|e| error::Commit::Parse { oid, source: e })
    }
}

impl Iterator for RevwalkCommits<'_> {
    type Item = Result<Commit, error::Commit>;

    fn next(&mut self) -> Option<Self::Item> {
        let oid = match self.oids.next()? {
            Ok(oid) => oid,
            Err(e) => return Some(Err(error::Commit::backend(e))),
        };
        Some(Self::read(self.backend, oid))
    }
}

/// Configure a [`raw::Revwalk`] from a [`RevwalkPlan`].
fn configure_revwalk(walk: &mut raw::Revwalk<'_>, plan: &RevwalkPlan) -> Result<(), error::Init> {
    let sort = match plan.sort_order() {
        SortOrder::Chronological { reverse: false } => git::raw::Sort::TIME,
        SortOrder::Topological { reverse: false } => git::raw::Sort::TOPOLOGICAL,
        SortOrder::Chronological { reverse: true } => git::raw::Sort::REVERSE,
        SortOrder::Topological { reverse: true } => {
            git::raw::Sort::TOPOLOGICAL | git::raw::Sort::REVERSE
        }
    };
    walk.set_sorting(sort).map_err(error::Init::backend)?;

    if let Some((from, to)) = plan.range_bounds() {
        walk.push_range(&format!("{from}..{to}"))
            .map_err(error::Init::backend)?;
    }

    for oid in plan.starts() {
        walk.push((*oid).into()).map_err(error::Init::backend)?;
    }

    for oid in plan.hidden() {
        walk.hide((*oid).into()).map_err(error::Init::backend)?;
    }

    Ok(())
}

impl Revwalk for raw::Repository {
    type RevwalkOids<'a> = RevwalkOids<'a>;
    type RevwalkCommits<'a> = RevwalkCommits<'a>;

    fn revwalk_oids<'a>(
        &'a self,
        plan: &RevwalkPlan,
    ) -> Result<Self::RevwalkOids<'a>, error::Init> {
        let mut walk = self.revwalk().map_err(error::Init::backend)?;
        configure_revwalk(&mut walk, plan)?;
        Ok(RevwalkOids { inner: walk })
    }

    fn revwalk_commits<'a>(
        &'a self,
        plan: &RevwalkPlan,
    ) -> Result<Self::RevwalkCommits<'a>, error::Init> {
        let mut walk = self.revwalk().map_err(error::Init::backend)?;
        configure_revwalk(&mut walk, plan)?;
        Ok(RevwalkCommits {
            oids: RevwalkOids { inner: walk },
            backend: self,
        })
    }
}
