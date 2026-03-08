use std::ops::ControlFlow;
use std::str::FromStr;

use sqlite as sql;
use thiserror::Error;

use crate::cob;
use crate::cob::cache::{self, StoreReader};
use crate::cob::cache::{Remove, StoreWriter, Update};
use crate::cob::store;
use crate::cob::store::access::{ReadOnly, WriteAs};
use crate::cob::{Label, ObjectId, TypeName};
use crate::git;
use crate::prelude::RepoId;
use crate::storage::{HasRepoId, ReadRepository, RepositoryError, SignRepository, WriteRepository};

use super::{
    ByRevision, MergeTarget, NodeId, Patch, PatchCounts, PatchId, PatchMut, Revision, RevisionId,
    State, Status,
};

/// A set of read-only methods for a [`Patch`] store.
pub trait Patches {
    type Error: std::error::Error + Send + Sync + 'static;

    /// An iterator for returning a set of patches from the store.
    type Iter<'a>: Iterator<Item = Result<(PatchId, Patch), Self::Error>> + 'a
    where
        Self: 'a;

    /// Get the `Patch`, identified by `id`, returning `None` if it
    /// was not found.
    fn get(&self, id: &PatchId) -> Result<Option<Patch>, Self::Error>;

    /// Get the `Patch` and its `Revision`, identified by the revision
    /// `id`, returning `None` if it was not found.
    fn find_by_revision(&self, id: &RevisionId) -> Result<Option<ByRevision>, Self::Error>;

    /// List all patches that are in the store.
    fn list(&self) -> Result<Self::Iter<'_>, Self::Error>;

    /// List all patches in the store that match the provided
    /// `status`.
    ///
    /// Also see [`Patches::opened`], [`Patches::archived`],
    /// [`Patches::drafted`], [`Patches::merged`].
    fn list_by_status(&self, status: &Status) -> Result<Self::Iter<'_>, Self::Error>;

    /// Get the [`PatchCounts`] of all the patches in the store.
    fn counts(&self) -> Result<PatchCounts, Self::Error>;

    /// List all opened patches in the store.
    fn opened(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.list_by_status(&Status::Open)
    }

    /// List all archived patches in the store.
    fn archived(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.list_by_status(&Status::Archived)
    }

    /// List all drafted patches in the store.
    fn drafted(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.list_by_status(&Status::Draft)
    }

    /// List all merged patches in the store.
    fn merged(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.list_by_status(&Status::Merged)
    }

    /// Returns `true` if there are no patches in the store.
    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.counts()?.total() == 0)
    }
}

/// [`Patches`] store that can also [`Update`] and [`Remove`]
/// [`Patch`] in/from the store.
pub trait PatchesMut: Patches + Update<Patch> + Remove<Patch> {}

impl<T> PatchesMut for T where T: Patches + Update<Patch> + Remove<Patch> {}

/// A `Patch` store that relies on the `cache` for reads and as a
/// write-through cache.
///
/// The `store` is used for the main storage when performing a
/// write-through. It is also used for identifying which `RepoId` is
/// being used for the `cache`.
pub struct Cache<'a, Repo, Access, C> {
    pub(super) store: super::Patches<'a, Repo, Access>,
    pub(super) cache: C,
}

impl<'a, Repo, Access, C> Cache<'a, Repo, Access, C> {
    pub fn new(store: super::Patches<'a, Repo, Access>, cache: C) -> Self {
        Self { store, cache }
    }
}

impl<'a, Repo, Access, C> HasRepoId for Cache<'a, Repo, Access, C>
where
    Repo: HasRepoId,
{
    fn rid(&self) -> RepoId {
        self.store.rid()
    }
}

impl<'a, 'b, Repo, Signer, C> Cache<'a, Repo, WriteAs<'b, Signer>, C>
where
    Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
    Signer: crypto::signature::Signer<crypto::Signature>,
    Signer: crypto::signature::Signer<crypto::ssh::ExtendedSignature>,
    Signer: crypto::signature::Verifier<crypto::Signature>,
{
    /// Create a new [`Patch`] using the [`super::Patches`] as the
    /// main storage, and writing the update to the `cache`.
    pub fn create<'g>(
        &'g mut self,
        title: cob::Title,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
    ) -> Result<PatchMut<'a, 'b, 'g, Repo, Signer, C>, super::Error>
    where
        Repo: WriteRepository + cob::Store<Namespace = NodeId>,
        C: Update<Patch>,
    {
        self.store.create(
            title,
            description,
            target,
            base,
            oid,
            labels,
            &mut self.cache,
        )
    }

    /// Create a new [`Patch`], in a draft state, using the
    /// [`super::Patches`] as the main storage, and writing the update
    /// to the `cache`.
    pub fn draft<'g>(
        &'g mut self,
        title: cob::Title,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
    ) -> Result<PatchMut<'a, 'b, 'g, Repo, Signer, C>, super::Error>
    where
        Repo: WriteRepository + cob::Store<Namespace = NodeId>,
        C: Update<Patch>,
    {
        self.store.draft(
            title,
            description,
            target,
            base,
            oid,
            labels,
            &mut self.cache,
        )
    }

    /// Remove the given `id` from the [`super::Patches`] storage, and
    /// removing the entry from the `cache`.
    pub fn remove(&mut self, id: &PatchId) -> Result<(), super::Error>
    where
        Repo: ReadRepository + SignRepository + cob::Store<Namespace = NodeId>,
        C: Remove<Patch>,
    {
        self.store.raw.remove(id)?;
        self.cache
            .remove(id)
            .map_err(|e| super::Error::CacheRemove {
                id: *id,
                err: e.into(),
            })?;
        Ok(())
    }
}

impl<'a, Repo, Access, C> Cache<'a, Repo, Access, C>
where
    Access: cob::store::access::Access,
{
    /// Read the given `id` from the [`super::Patches`] store and
    /// writing it to the `cache`.
    pub fn write(&mut self, id: &PatchId) -> Result<(), super::Error>
    where
        Repo: ReadRepository + cob::Store<Namespace = NodeId>,
        C: Update<Patch>,
    {
        let issue = self
            .store
            .get(id)?
            .ok_or_else(|| store::Error::NotFound((*super::TYPENAME).clone(), *id))?;
        self.update(&self.rid(), id, &issue)
            .map_err(|e| super::Error::CacheUpdate {
                id: *id,
                err: e.into(),
            })?;
        Ok(())
    }

    /// Read all the patches from the [`super::Patches`] store and
    /// writing them to `cache`.
    ///
    /// The `callback` is used for reporting success, failures, and
    /// progress to the caller. The caller may also decide to continue
    /// or break from the process.
    pub fn write_all(
        &mut self,
        callback: impl Fn(&Result<(PatchId, Patch), store::Error>, &cache::Progress) -> ControlFlow<()>,
    ) -> Result<(), super::Error>
    where
        Repo: ReadRepository + cob::Store<Namespace = NodeId>,
        C: Update<Patch> + Remove<Patch>,
    {
        // Start by clearing the cache. This will get rid of patches that are cached but
        // no longer exist in storage.
        self.remove_all(&self.rid())
            .map_err(|e| super::Error::CacheRemoveAll { err: e.into() })?;

        let patches = self.store.all()?;
        let mut progress = cache::Progress::new(patches.len());
        for patch in self.store.all()? {
            progress.inc();
            match callback(&patch, &progress) {
                ControlFlow::Continue(()) => match patch {
                    Ok((id, patch)) => {
                        self.update(&self.rid(), &id, &patch)
                            .map_err(|e| super::Error::CacheUpdate { id, err: e.into() })?;
                    }
                    Err(_) => continue,
                },
                ControlFlow::Break(()) => break,
            }
        }
        Ok(())
    }
}

impl<'a, Repo> Cache<'a, Repo, ReadOnly, StoreReader> {
    pub fn reader(store: super::Patches<'a, Repo, ReadOnly>, cache: StoreReader) -> Self {
        Self { store, cache }
    }
}

impl<'a, Repo, Access> Cache<'a, Repo, Access, StoreWriter> {
    pub fn open(store: super::Patches<'a, Repo, Access>, cache: StoreWriter) -> Self {
        Self { store, cache }
    }
}

impl<'a, 'b, Repo, Signer> Cache<'a, Repo, WriteAs<'b, Signer>, StoreWriter>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
{
    /// Get the [`PatchMut`], identified by `id`, using the
    /// `StoreWriter` for retrieving the `Patch`.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<PatchMut<'a, 'b, 'g, Repo, Signer, StoreWriter>, Error> {
        let patch = Patches::get(self, id)?
            .ok_or_else(move || Error::NotFound(super::TYPENAME.clone(), *id))?;

        Ok(PatchMut {
            id: *id,
            patch,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }
}

impl<'a, 'b, Repo, Signer> Cache<'a, Repo, WriteAs<'b, Signer>, cache::NoCache>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
{
    /// Get a `Cache` that does no write-through modifications and
    /// uses the [`super::Patches`] store for all reads and writes.
    pub fn no_cache(repository: &'a Repo, signer: &'b Signer) -> Result<Self, RepositoryError> {
        let store = super::Patches::open(repository, WriteAs::new(signer))?;
        Ok(Self {
            store,
            cache: cache::NoCache,
        })
    }

    /// Get the [`PatchMut`], identified by `id`.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<PatchMut<'a, 'b, 'g, Repo, Signer, cache::NoCache>, super::Error> {
        let patch = self
            .store
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(super::TYPENAME.clone(), *id))?;

        Ok(PatchMut {
            id: *id,
            patch,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }
}

impl<'a, Repo, Access, C> cache::Update<Patch> for Cache<'a, Repo, Access, C>
where
    C: cache::Update<Patch>,
{
    type Out = <C as cache::Update<Patch>>::Out;
    type UpdateError = <C as cache::Update<Patch>>::UpdateError;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &radicle_cob::ObjectId,
        object: &Patch,
    ) -> Result<Self::Out, Self::UpdateError> {
        self.cache.update(rid, id, object)
    }
}

impl<'a, Repo, Access, C> cache::Remove<Patch> for Cache<'a, Repo, Access, C>
where
    C: cache::Remove<Patch>,
{
    type Out = <C as cache::Remove<Patch>>::Out;
    type RemoveError = <C as cache::Remove<Patch>>::RemoveError;

    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        self.cache.remove(id)
    }

    fn remove_all(&mut self, rid: &RepoId) -> Result<Self::Out, Self::RemoveError> {
        self.cache.remove_all(rid)
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

impl Update<Patch> for StoreWriter {
    type Out = bool;
    type UpdateError = UpdateError;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &Patch,
    ) -> Result<Self::Out, Self::UpdateError> {
        let mut stmt = self.db.prepare(
            "INSERT INTO patches (id, repo, patch)
             VALUES (?1, ?2, ?3)
             ON CONFLICT DO UPDATE
             SET patch = (?3)",
        )?;

        stmt.bind((1, sql::Value::String(id.to_string())))?;
        stmt.bind((2, rid))?;
        stmt.bind((3, sql::Value::String(serde_json::to_string(&object)?)))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }
}

impl Remove<Patch> for StoreWriter {
    type Out = bool;
    type RemoveError = sql::Error;

    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        let mut stmt = self.db.prepare(
            "DELETE FROM patches
             WHERE id = ?1",
        )?;

        stmt.bind((1, sql::Value::String(id.to_string())))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn remove_all(&mut self, rid: &RepoId) -> Result<Self::Out, Self::RemoveError> {
        let mut stmt = self.db.prepare(
            "DELETE FROM patches
             WHERE repo = ?1",
        )?;

        stmt.bind((1, rid))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
    #[error(transparent)]
    ObjectId(#[from] cob::object::ParseObjectId),
    #[error("object {0} failed to parse: {1}")]
    Object(ObjectId, serde_json::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

/// Iterator that returns a set of patches based on an SQL query.
///
/// The query is expected to return rows with columns identified by
/// the `id` and `patch` names.
pub struct PatchesIter<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl PatchesIter<'_> {
    fn parse_row(row: sql::Row) -> Result<(PatchId, Patch), Error> {
        let id = PatchId::from_str(row.try_read::<&str, _>("id")?)?;
        let patch = serde_json::from_str::<Patch>(row.try_read::<&str, _>("patch")?)
            .map_err(|e| Error::Object(id, e))?;
        Ok((id, patch))
    }
}

impl Iterator for PatchesIter<'_> {
    type Item = Result<(PatchId, Patch), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(PatchesIter::parse_row))
    }
}

impl<'a, Repo, Access> Patches for Cache<'a, Repo, Access, StoreReader>
where
    Repo: HasRepoId,
{
    type Error = Error;
    type Iter<'b>
        = PatchesIter<'b>
    where
        Self: 'b;

    fn get(&self, id: &PatchId) -> Result<Option<Patch>, Self::Error> {
        query::get(&self.cache.db, &self.rid(), id)
    }

    fn find_by_revision(&self, id: &RevisionId) -> Result<Option<ByRevision>, Error> {
        query::find_by_revision(&self.cache.db, &self.rid(), id)
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        query::list(&self.cache.db, &self.rid())
    }

    fn list_by_status(&self, status: &Status) -> Result<Self::Iter<'_>, Self::Error> {
        query::list_by_status(&self.cache.db, &self.rid(), status)
    }

    fn counts(&self) -> Result<PatchCounts, Self::Error> {
        query::counts(&self.cache.db, &self.rid())
    }
}

pub struct NoCacheIter<'a> {
    inner: Box<dyn Iterator<Item = Result<(PatchId, Patch), super::Error>> + 'a>,
}

impl Iterator for NoCacheIter<'_> {
    type Item = Result<(PatchId, Patch), super::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, Repo, Access> Patches for Cache<'a, Repo, Access, cache::NoCache>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
    Access: store::access::Access,
{
    type Error = super::Error;
    type Iter<'b>
        = NoCacheIter<'b>
    where
        Self: 'b;

    fn get(&self, id: &PatchId) -> Result<Option<Patch>, Self::Error> {
        self.store.get(id).map_err(super::Error::from)
    }

    fn find_by_revision(&self, id: &RevisionId) -> Result<Option<ByRevision>, Self::Error> {
        self.store.find_by_revision(id)
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.store
            .all()
            .map(|inner| NoCacheIter {
                inner: Box::new(inner.into_iter().map(|res| res.map_err(super::Error::from))),
            })
            .map_err(super::Error::from)
    }

    fn list_by_status(&self, status: &Status) -> Result<Self::Iter<'_>, Self::Error> {
        let status = *status;
        self.store
            .all()
            .map(move |inner| NoCacheIter {
                inner: Box::new(inner.into_iter().filter_map(move |res| {
                    match res {
                        Ok((id, patch)) => (status == Status::from(&patch.state))
                            .then_some((id, patch))
                            .map(Ok),
                        Err(e) => Some(Err(e.into())),
                    }
                })),
            })
            .map_err(super::Error::from)
    }

    fn counts(&self) -> Result<PatchCounts, Self::Error> {
        self.store.counts().map_err(super::Error::from)
    }
}

impl<'a, Repo, Access> Patches for Cache<'a, Repo, Access, StoreWriter>
where
    Repo: HasRepoId + cob::Store<Namespace = NodeId>,
    Access: store::access::Access,
{
    type Error = Error;
    type Iter<'b>
        = PatchesIter<'b>
    where
        Self: 'b;

    fn get(&self, id: &PatchId) -> Result<Option<Patch>, Self::Error> {
        query::get(&self.cache.db, &self.rid(), id)
    }

    fn find_by_revision(&self, id: &RevisionId) -> Result<Option<ByRevision>, Error> {
        query::find_by_revision(&self.cache.db, &self.rid(), id)
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        query::list(&self.cache.db, &self.rid())
    }

    fn list_by_status(&self, status: &Status) -> Result<Self::Iter<'_>, Self::Error> {
        query::list_by_status(&self.cache.db, &self.rid(), status)
    }

    fn counts(&self) -> Result<PatchCounts, Self::Error> {
        query::counts(&self.cache.db, &self.rid())
    }
}

/// Helper SQL queries for [ `Patches`] trait implementations.
mod query {
    use sqlite as sql;

    use crate::patch::Status;

    use super::*;

    pub(super) fn get(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
        id: &PatchId,
    ) -> Result<Option<Patch>, Error> {
        let key = sql::Value::String(id.to_string());
        let mut stmt = db.prepare(
            "SELECT patch
             FROM patches
             WHERE id = ?1 AND repo = ?2",
        )?;

        stmt.bind((1, key))?;
        stmt.bind((2, rid))?;

        match stmt.into_iter().next().transpose()? {
            None => Ok(None),
            Some(row) => {
                let patch = row.try_read::<&str, _>("patch")?;
                let patch = serde_json::from_str(patch).map_err(|e| Error::Object(*id, e))?;
                Ok(Some(patch))
            }
        }
    }

    pub(super) fn find_by_revision(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
        id: &RevisionId,
    ) -> Result<Option<ByRevision>, Error> {
        let revision_id = *id;
        let mut stmt = db.prepare(
            "SELECT patches.id, patch, revisions.value AS revision
             FROM patches, json_tree(patches.patch, '$.revisions') AS revisions
             WHERE repo = ?1
             AND revisions.key = ?2
            ",
        )?;
        stmt.bind((1, rid))?;
        stmt.bind((2, sql::Value::String(id.to_string())))?;

        match stmt.into_iter().next().transpose()? {
            None => Ok(None),
            Some(row) => {
                let id = PatchId::from_str(row.try_read::<&str, _>("id")?)?;
                let patch = serde_json::from_str::<Patch>(row.try_read::<&str, _>("patch")?)
                    .map_err(|e| Error::Object(id, e))?;
                let revision =
                    serde_json::from_str::<Revision>(row.try_read::<&str, _>("revision")?)?;
                Ok(Some(ByRevision {
                    id,
                    patch,
                    revision_id,
                    revision,
                }))
            }
        }
    }

    pub(super) fn list<'a>(
        db: &'a sql::ConnectionThreadSafe,
        rid: &RepoId,
    ) -> Result<PatchesIter<'a>, Error> {
        let mut stmt = db.prepare(
            "SELECT id, patch
             FROM patches
             WHERE repo = ?1
             ORDER BY id
            ",
        )?;
        stmt.bind((1, rid))?;
        Ok(PatchesIter {
            inner: stmt.into_iter(),
        })
    }

    pub(super) fn list_by_status<'a>(
        db: &'a sql::ConnectionThreadSafe,
        rid: &RepoId,
        filter: &Status,
    ) -> Result<PatchesIter<'a>, Error> {
        let mut stmt = db.prepare(
            "SELECT patches.id, patch
             FROM patches
             WHERE repo = ?1
             AND patch->>'$.state.status' = ?2
             ORDER BY id
            ",
        )?;
        stmt.bind((1, rid))?;
        stmt.bind((2, sql::Value::String(filter.to_string())))?;
        Ok(PatchesIter {
            inner: stmt.into_iter(),
        })
    }

    pub(super) fn counts(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
    ) -> Result<PatchCounts, Error> {
        let mut stmt = db.prepare(
            "SELECT
                 patch->'$.state' AS state,
                 COUNT(*) AS count
             FROM patches
             WHERE repo = ?1
             GROUP BY patch->'$.state.status'",
        )?;
        stmt.bind((1, rid))?;

        stmt.into_iter()
            .try_fold(PatchCounts::default(), |mut counts, row| {
                let row = row?;
                let count = row.try_read::<i64, _>("count")? as usize;
                let status = serde_json::from_str::<State>(row.try_read::<&str, _>("state")?)?;
                match status {
                    State::Draft => counts.draft += count,
                    State::Open { .. } => counts.open += count,
                    State::Archived => counts.archived += count,
                    State::Merged { .. } => counts.merged += count,
                }
                Ok(counts)
            })
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::NonZeroU8;
    use std::str::FromStr;

    use amplify::Wrapper;
    use radicle_cob::ObjectId;

    use crate::cob::cache::{Store, Update, Write};
    use crate::cob::store::access::ReadOnly;
    use crate::cob::thread::{Comment, Thread};
    use crate::cob::{Author, Title, migrate};
    use crate::patch::{
        ByRevision, MergeTarget, Patch, PatchCounts, PatchId, Revision, RevisionId, State, Status,
    };
    use crate::prelude::Did;
    use crate::profile::env;
    use crate::storage::HasRepoId as _;
    use crate::test::arbitrary;
    use crate::test::storage::MockRepository;

    use super::{Cache, Patches};

    fn memory<'a>(store: &'a MockRepository) -> Cache<'a, MockRepository, ReadOnly, Store<Write>> {
        let store = super::super::Patches::open(store, ReadOnly).unwrap();
        let cache = Store::<Write>::memory()
            .unwrap()
            .with_migrations(migrate::ignore)
            .unwrap();
        Cache { store, cache }
    }

    fn revision() -> (RevisionId, Revision) {
        let author = arbitrary::r#gen::<Did>(1);
        let description = arbitrary::r#gen::<String>(1);
        let base = arbitrary::oid();
        let oid = arbitrary::oid();
        let timestamp = env::local_time();
        let resolves = BTreeSet::new();
        let id = RevisionId::from(arbitrary::oid());
        let mut revision = Revision::new(
            id,
            Author { id: author },
            description,
            base,
            oid,
            timestamp.into(),
            resolves,
        );
        let comment = Comment::new(
            *author,
            "#1 comment".to_string(),
            None,
            None,
            vec![],
            timestamp.into(),
        );
        let thread = Thread::new(arbitrary::oid(), comment);
        revision.discussion = thread;
        (id, revision)
    }

    #[test]
    fn test_is_empty() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        assert!(cache.is_empty().unwrap());

        let patch = Patch::new(
            Title::new("Patch #1").unwrap(),
            MergeTarget::Delegates,
            revision(),
        );
        let id = ObjectId::from_str("47799cbab2eca047b6520b9fce805da42b49ecab").unwrap();
        cache.update(&cache.rid(), &id, &patch).unwrap();

        let patch = Patch {
            state: State::Archived,
            ..Patch::new(
                Title::new("Patch #2").unwrap(),
                MergeTarget::Delegates,
                revision(),
            )
        };
        let id = ObjectId::from_str("ae981ded6ed2ed2cdba34c8603714782667f18a3").unwrap();
        cache.update(&cache.rid(), &id, &patch).unwrap();

        assert!(!cache.is_empty().unwrap())
    }

    #[test]
    fn test_counts() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let n_open = arbitrary::r#gen::<u8>(0);
        let n_draft = arbitrary::r#gen::<u8>(1);
        let n_archived = arbitrary::r#gen::<u8>(1);
        let n_merged = arbitrary::r#gen::<u8>(1);
        let open_ids = (0..n_open)
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let draft_ids = (0..n_draft)
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let archived_ids = (0..n_archived)
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let merged_ids = (0..n_merged)
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();

        for id in open_ids.iter() {
            let patch = Patch::new(
                Title::new(&id.to_string()).unwrap(),
                MergeTarget::Delegates,
                revision(),
            );
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
        }

        for id in draft_ids.iter() {
            let patch = Patch {
                state: State::Draft,
                ..Patch::new(
                    Title::new(&id.to_string()).unwrap(),
                    MergeTarget::Delegates,
                    revision(),
                )
            };
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
        }

        for id in archived_ids.iter() {
            let patch = Patch {
                state: State::Archived,
                ..Patch::new(
                    Title::new(&id.to_string()).unwrap(),
                    MergeTarget::Delegates,
                    revision(),
                )
            };
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
        }

        for id in merged_ids.iter() {
            let patch = Patch {
                state: State::Merged {
                    revision: arbitrary::oid().into(),
                    commit: arbitrary::oid(),
                },
                ..Patch::new(
                    Title::new(&id.to_string()).unwrap(),
                    MergeTarget::Delegates,
                    revision(),
                )
            };
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
        }

        assert_eq!(
            cache.counts().unwrap(),
            PatchCounts {
                open: open_ids.len(),
                draft: draft_ids.len(),
                archived: archived_ids.len(),
                merged: merged_ids.len(),
            }
        );
    }

    #[test]
    fn test_get() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::r#gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let missing = (0..arbitrary::r#gen::<u8>(2))
            .filter_map(|_| {
                let id = PatchId::from(arbitrary::oid());
                (!ids.contains(&id)).then_some(id)
            })
            .collect::<BTreeSet<PatchId>>();
        let mut patches = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let patch = Patch::new(
                Title::new(&id.to_string()).unwrap(),
                MergeTarget::Delegates,
                revision(),
            );
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
            patches.push((*id, patch));
        }

        for (id, patch) in patches.into_iter() {
            assert_eq!(Some(patch), cache.get(&id).unwrap());
        }

        for id in &missing {
            assert_eq!(cache.get(id).unwrap(), None);
        }
    }

    #[test]
    fn test_find_by_revision() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let patch_id = PatchId::from(arbitrary::oid());
        let revisions = (0..arbitrary::r#gen::<NonZeroU8>(1).into())
            .map(|_| revision())
            .collect::<BTreeMap<RevisionId, Revision>>();
        let (rev_id, rev) = revisions
            .iter()
            .next()
            .expect("at least one revision should have been created");
        let mut patch = Patch::new(
            Title::new(&patch_id.to_string()).unwrap(),
            MergeTarget::Delegates,
            (*rev_id, rev.clone()),
        );
        let timeline = revisions.keys().copied().collect::<Vec<_>>();
        patch
            .timeline
            .extend(timeline.iter().map(|id| id.into_inner()));
        patch
            .revisions
            .extend(revisions.iter().map(|(id, rev)| (*id, Some(rev.clone()))));
        cache
            .update(&cache.rid(), &PatchId::from(*patch_id), &patch)
            .unwrap();

        for entry in timeline {
            let rev = revisions.get(&entry).unwrap().clone();
            assert_eq!(
                Some(ByRevision {
                    id: patch_id,
                    patch: patch.clone(),
                    revision_id: entry,
                    revision: rev
                }),
                cache.find_by_revision(&entry).unwrap()
            );
        }
    }

    #[test]
    fn test_list() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::r#gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let mut patches = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let patch = Patch::new(
                Title::new(&id.to_string()).unwrap(),
                MergeTarget::Delegates,
                revision(),
            );
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
            patches.push((*id, patch));
        }

        let mut list = cache
            .list()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        list.sort_by_key(|(id, _)| *id);
        patches.sort_by_key(|(id, _)| *id);
        assert_eq!(patches, list);
    }

    #[test]
    fn test_list_by_status() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::r#gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let mut patches = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let patch = Patch::new(
                Title::new(&id.to_string()).unwrap(),
                MergeTarget::Delegates,
                revision(),
            );
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
            patches.push((*id, patch));
        }

        let mut list = cache
            .list_by_status(&Status::Open)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        list.sort_by_key(|(id, _)| *id);
        patches.sort_by_key(|(id, _)| *id);
        assert_eq!(patches, list);
    }

    #[test]
    fn test_remove() {
        let repo = arbitrary::r#gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::r#gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();

        for id in ids.iter() {
            let patch = Patch::new(
                Title::new(&id.to_string()).unwrap(),
                MergeTarget::Delegates,
                revision(),
            );
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
            assert_eq!(Some(patch), cache.get(id).unwrap());
            super::Remove::remove(&mut cache, id).unwrap();
            assert_eq!(None, cache.get(id).unwrap());
        }
    }
}
