use std::str::FromStr;

use sqlite as sql;
use thiserror::Error;

use crate::cob;
use crate::cob::cache::{self, StoreReader};
use crate::cob::cache::{Remove, StoreWriter, Update};
use crate::cob::store;
use crate::cob::{Label, ObjectId, TypeName};
use crate::crypto::Signer;
use crate::git;
use crate::prelude::RepoId;
use crate::sql::transaction;
use crate::storage::{ReadRepository, RepositoryError, SignRepository, WriteRepository};

use super::{
    ByRevision, MergeTarget, Patch, PatchCounts, PatchId, PatchMut, Revision, RevisionId, State,
};

pub trait Patches {
    type Error: std::error::Error + Send + Sync + 'static;
    type Iter<'a>: Iterator<Item = Result<(PatchId, Patch), Self::Error>> + 'a
    where
        Self: 'a;

    fn get(&self, id: &PatchId) -> Result<Option<Patch>, Self::Error>;
    fn find_by_revision(&self, id: &RevisionId) -> Result<Option<ByRevision>, Self::Error>;
    fn list(&self) -> Result<Self::Iter<'_>, Self::Error>;
    fn counts(&self) -> Result<PatchCounts, Self::Error>;

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.counts()?.total() == 0)
    }
}

pub trait PatchesMut: Patches + Update<Patch> + Remove<Patch> {}

impl<T> PatchesMut for T where T: Patches + Update<Patch> + Remove<Patch> {}

pub struct Cache<'a, R, C> {
    pub(super) store: super::Patches<'a, R>,
    pub(super) cache: C,
}

impl<'a, R, C> Cache<'a, R, C>
where
    R: ReadRepository,
{
    pub fn rid(&self) -> RepoId {
        self.store.raw.as_ref().id()
    }
}

impl<'a, R> Cache<'a, R, StoreReader>
where
    R: ReadRepository + cob::Store,
{
    pub fn reader(repository: &'a R, cache: StoreReader) -> Result<Self, RepositoryError> {
        let store = super::Patches::open(repository)?;
        Ok(Self { store, cache })
    }
}

impl<'a, R> Cache<'a, R, StoreWriter>
where
    R: ReadRepository + cob::Store,
{
    pub fn open(repository: &'a R, cache: StoreWriter) -> Result<Self, RepositoryError> {
        let store = super::Patches::open(repository)?;
        Ok(Self { store, cache })
    }

    pub fn create<'g, G>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
        signer: &G,
    ) -> Result<PatchMut<'a, 'g, R, StoreWriter>, super::Error>
    where
        R: WriteRepository,
        G: Signer,
    {
        self.store.create(
            title,
            description,
            target,
            base,
            oid,
            labels,
            &mut self.cache,
            signer,
        )
    }

    pub fn draft<'g, G>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        target: MergeTarget,
        base: impl Into<git::Oid>,
        oid: impl Into<git::Oid>,
        labels: &[Label],
        signer: &G,
    ) -> Result<PatchMut<'a, 'g, R, StoreWriter>, super::Error>
    where
        R: WriteRepository,
        G: Signer,
    {
        self.store.draft(
            title,
            description,
            target,
            base,
            oid,
            labels,
            &mut self.cache,
            signer,
        )
    }

    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<PatchMut<'a, 'g, R, StoreWriter>, Error> {
        let patch = Patches::get(self, id)?
            .ok_or_else(move || Error::NotFound(super::TYPENAME.clone(), *id))?;

        Ok(PatchMut {
            id: *id,
            patch,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }

    pub fn remove<G>(&mut self, id: &PatchId, signer: &G) -> Result<(), store::Error>
    where
        G: Signer,
        R: SignRepository,
    {
        self.store.remove(id, &mut self.cache, signer)
    }
}

impl<'a, R, C> cache::Update<Patch> for Cache<'a, R, C>
where
    C: cache::Update<Patch>,
{
    type Out = <C as cache::Update<Patch>>::Out;
    type UpdateError = <C as cache::Update<Patch>>::UpdateError;

    fn update(
        &mut self,
        rid: &crate::prelude::RepoId,
        id: &radicle_cob::ObjectId,
        object: &Patch,
    ) -> Result<Self::Out, Self::UpdateError> {
        self.cache.update(rid, id, object)
    }
}

impl<'a, R, C> cache::Remove<Patch> for Cache<'a, R, C>
where
    C: cache::Remove<Patch>,
{
    type Out = <C as cache::Remove<Patch>>::Out;
    type RemoveError = <C as cache::Remove<Patch>>::RemoveError;

    fn remove(&mut self, rid: &RepoId, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        self.cache.remove(rid, id)
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
        transaction::<_, UpdateError>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "INSERT INTO patches (id, repo_id, patch)
                  VALUES (?1, ?2, ?3)
                  ON CONFLICT DO UPDATE
                  SET patch =  (?3)",
            )?;

            stmt.bind((1, sql::Value::String(id.to_string())))?;
            stmt.bind((2, rid))?;
            stmt.bind((3, sql::Value::String(serde_json::to_string(&object)?)))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

impl Remove<Patch> for StoreWriter {
    type Out = bool;
    type RemoveError = sql::Error;

    fn remove(&mut self, rid: &RepoId, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        transaction::<_, sql::Error>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "DELETE FROM patches
                  WHERE  repo_id =  ?1
                  AND id = ?2",
            )?;

            stmt.bind((1, rid))?;
            stmt.bind((2, sql::Value::String(id.to_string())))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
    #[error(transparent)]
    Object(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

pub struct PatchesIter<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> PatchesIter<'a> {
    fn parse_row(row: sql::Row) -> Result<(PatchId, Patch), Error> {
        let id = PatchId::from_str(row.read::<&str, _>("id"))?;
        let patch = serde_json::from_str::<Patch>(row.read::<&str, _>("patch"))?;
        Ok((id, patch))
    }
}

impl<'a> Iterator for PatchesIter<'a> {
    type Item = Result<(PatchId, Patch), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(PatchesIter::parse_row))
    }
}

impl<'a, R> Patches for Cache<'a, R, StoreReader>
where
    R: ReadRepository,
{
    type Error = Error;
    type Iter<'b> = PatchesIter<'b>
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

    fn counts(&self) -> Result<PatchCounts, Self::Error> {
        query::counts(&self.cache.db, &self.rid())
    }
}

impl<'a, R> Patches for Cache<'a, R, StoreWriter>
where
    R: ReadRepository,
{
    type Error = Error;
    type Iter<'b> = PatchesIter<'b>
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

    fn counts(&self) -> Result<PatchCounts, Self::Error> {
        query::counts(&self.cache.db, &self.rid())
    }
}

mod query {
    use sqlite as sql;

    use super::*;

    pub(super) fn get(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
        id: &PatchId,
    ) -> Result<Option<Patch>, Error> {
        let id = sql::Value::String(id.to_string());
        let mut stmt = db.prepare(
            "SELECT patch
             FROM patches
             WHERE id = ?1 and repo_id = ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, rid))?;

        match stmt.into_iter().next().transpose()? {
            None => Ok(None),
            Some(row) => {
                let patch = row.read::<&str, _>("patch");
                let patch = serde_json::from_str(patch)?;
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
              WHERE repo_id = ?1
              AND revisions.key = ?2
            ",
        )?;
        stmt.bind((1, rid))?;
        stmt.bind((2, sql::Value::String(id.to_string())))?;

        match stmt.into_iter().next().transpose()? {
            None => Ok(None),
            Some(row) => {
                let id = PatchId::from_str(row.read::<&str, _>("id"))?;
                let patch = serde_json::from_str::<Patch>(row.read::<&str, _>("patch"))?;
                let revision = serde_json::from_str::<Revision>(row.read::<&str, _>("revision"))?;
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
            "SELECT id, patch FROM patches
              WHERE repo_id = ?1
            ",
        )?;
        stmt.bind((1, rid))?;
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
                 patch->'$.state' as state,
                 COUNT(*) as count
             FROM patches
             WHERE repo_id = ?1
             GROUP BY patch->'$.state.status'",
        )?;
        stmt.bind((1, rid))?;

        stmt.into_iter()
            .try_fold(PatchCounts::default(), |mut counts, row| {
                let row = row?;
                let count = row.read::<i64, _>("count") as usize;
                let status = serde_json::from_str::<State>(row.read::<&str, _>("state"))?;
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
    use crate::cob::thread::{Comment, Thread};
    use crate::cob::{self, Author, Timestamp};
    use crate::patch::{
        ByRevision, MergeTarget, Patch, PatchCounts, PatchId, Revision, RevisionId, State,
    };
    use crate::prelude::Did;
    use crate::test::arbitrary;
    use crate::test::storage::{MockRepository, ReadRepository as _};

    use super::{Cache, Patches};

    fn memory(repo: &MockRepository) -> Cache<'_, MockRepository, Store<Write>> {
        let store = cob::patch::Patches::open(repo).unwrap();
        let cache = Store::<Write>::memory().unwrap();
        Cache { store, cache }
    }

    fn revision() -> (RevisionId, Revision) {
        let author = arbitrary::gen::<Did>(1);
        let description = arbitrary::gen::<String>(1);
        let base = arbitrary::oid();
        let oid = arbitrary::oid();
        let timestamp = Timestamp::now();
        let resolves = BTreeSet::new();
        let mut revision = Revision::new(
            Author { id: author },
            description,
            base,
            oid,
            timestamp,
            resolves,
        );
        let comment = Comment::new(
            *author,
            "#1 comment".to_string(),
            None,
            None,
            vec![],
            Timestamp::now(),
        );
        let thread = Thread::new(arbitrary::oid(), comment);
        revision.discussion = thread;
        let id = RevisionId::from(arbitrary::oid());
        (id, revision)
    }

    #[test]
    fn test_is_empty() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        assert!(cache.is_empty().unwrap());

        let patch = Patch::new("Patch #1".to_string(), MergeTarget::Delegates, revision());
        let id = ObjectId::from_str("47799cbab2eca047b6520b9fce805da42b49ecab").unwrap();
        cache.update(&cache.rid(), &id, &patch).unwrap();

        let patch = Patch {
            state: State::Archived,
            ..Patch::new("Patch #2".to_string(), MergeTarget::Delegates, revision())
        };
        let id = ObjectId::from_str("ae981ded6ed2ed2cdba34c8603714782667f18a3").unwrap();
        cache.update(&cache.rid(), &id, &patch).unwrap();

        assert!(!cache.is_empty().unwrap())
    }

    #[test]
    fn test_counts() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let n_open = arbitrary::gen::<u8>(0);
        let n_draft = arbitrary::gen::<u8>(1);
        let n_archived = arbitrary::gen::<u8>(1);
        let n_merged = arbitrary::gen::<u8>(1);
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
            let patch = Patch::new(id.to_string(), MergeTarget::Delegates, revision());
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
        }

        for id in draft_ids.iter() {
            let patch = Patch {
                state: State::Draft,
                ..Patch::new(id.to_string(), MergeTarget::Delegates, revision())
            };
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
        }

        for id in archived_ids.iter() {
            let patch = Patch {
                state: State::Archived,
                ..Patch::new(id.to_string(), MergeTarget::Delegates, revision())
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
                ..Patch::new(id.to_string(), MergeTarget::Delegates, revision())
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
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let missing = (0..arbitrary::gen::<u8>(2))
            .filter_map(|_| {
                let id = PatchId::from(arbitrary::oid());
                (!ids.contains(&id)).then_some(id)
            })
            .collect::<BTreeSet<PatchId>>();
        let mut patches = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let patch = Patch::new(id.to_string(), MergeTarget::Delegates, revision());
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
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let patch_id = PatchId::from(arbitrary::oid());
        let revisions = (0..arbitrary::gen::<NonZeroU8>(1).into())
            .map(|_| revision())
            .collect::<BTreeMap<RevisionId, Revision>>();
        let (rev_id, rev) = revisions
            .iter()
            .next()
            .expect("at least one revision should have been created");
        let mut patch = Patch::new(
            patch_id.to_string(),
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
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();
        let mut patches = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let patch = Patch::new(id.to_string(), MergeTarget::Delegates, revision());
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
    fn test_remove() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| PatchId::from(arbitrary::oid()))
            .collect::<BTreeSet<PatchId>>();

        for id in ids.iter() {
            let patch = Patch::new(id.to_string(), MergeTarget::Delegates, revision());
            cache
                .update(&cache.rid(), &PatchId::from(*id), &patch)
                .unwrap();
            assert_eq!(Some(patch), cache.get(id).unwrap());
            super::Remove::remove(&mut cache, &repo.id(), id).unwrap();
            assert_eq!(None, cache.get(id).unwrap());
        }
    }
}
