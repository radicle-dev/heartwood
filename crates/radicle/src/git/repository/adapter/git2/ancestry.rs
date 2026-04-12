use radicle_oid::Oid;

use crate::git::raw;
use crate::git::repository::ancestry;
use crate::git::repository::ancestry::{AheadBehind, Ancestry};

use super::NotFound as _;

impl Ancestry for raw::Repository {
    fn merge_base(&self, a: Oid, b: Oid) -> Result<Option<Oid>, ancestry::error::MergeBase> {
        let odb = self.odb().map_err(ancestry::error::MergeBase::backend)?;

        if !odb.exists(a.into()) {
            return Err(ancestry::error::MergeBase::CommitNotFound { oid: a });
        }

        if !odb.exists(b.into()) {
            return Err(ancestry::error::MergeBase::CommitNotFound { oid: b });
        }

        self.merge_base(a.into(), b.into())
            .map(Oid::from)
            .or_is_not_found()
            .map_err(ancestry::error::MergeBase::backend)
    }

    fn is_ancestor(&self, ancestor: Oid, head: Oid) -> Result<bool, ancestry::error::IsAncestor> {
        let odb = self.odb().map_err(ancestry::error::IsAncestor::backend)?;

        if !odb.exists(ancestor.into()) {
            return Err(ancestry::error::IsAncestor::CommitNotFound { oid: ancestor });
        }

        if !odb.exists(head.into()) {
            return Err(ancestry::error::IsAncestor::CommitNotFound { oid: head });
        }

        self.graph_descendant_of(head.into(), ancestor.into())
            .map_err(ancestry::error::IsAncestor::backend)
    }

    fn ahead_behind(
        &self,
        commit: Oid,
        upstream: Oid,
    ) -> Result<AheadBehind, ancestry::error::AheadBehind> {
        let odb = self.odb().map_err(ancestry::error::AheadBehind::backend)?;

        if !odb.exists(commit.into()) {
            return Err(ancestry::error::AheadBehind::CommitNotFound { oid: commit });
        }

        if !odb.exists(upstream.into()) {
            return Err(ancestry::error::AheadBehind::CommitNotFound { oid: upstream });
        }

        let (ahead, behind) = self
            .graph_ahead_behind(commit.into(), upstream.into())
            .map_err(ancestry::error::AheadBehind::backend)?;
        Ok(AheadBehind { ahead, behind })
    }
}
