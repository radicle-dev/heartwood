use std::ops::ControlFlow;
use std::str::FromStr;

use sqlite as sql;
use thiserror::Error;

use crate::cob;
use crate::cob::cache;
use crate::cob::cache::{Remove, StoreReader, StoreWriter, Update};
use crate::cob::store;
use crate::cob::{ObjectId, TypeName};
use crate::crypto::{Signer, Verified};
use crate::prelude::{Doc, RepoId};
use crate::sql::transaction;
use crate::storage::HasRepoId;
use crate::storage::{ReadRepository, RepositoryError, WriteRepository};

use super::{Identity, IdentityMut};

/// A set of read-only methods for a [`Identity`] store.
pub trait Identities {
    type Error: std::error::Error + Send + Sync + 'static;

    /// An iterator for returning a set of identities from the store.
    type Iter<'a>: Iterator<Item = Result<(ObjectId, Identity), Self::Error>> + 'a
    where
        Self: 'a;

    /// Get the `Identity`, identified by `id`, returning `None` if it
    /// was not found.
    fn get(&self, id: &ObjectId) -> Result<Identity, Self::Error>;

    /// Get the `Identity`, using the internal `RepoId`, returning
    /// `None` if it was not found.
    fn load(&self) -> Result<Identity, Self::Error>;

    /// List all identities that are in the store.
    fn list(&self) -> Result<Self::Iter<'_>, Self::Error>;
}

/// [`Identities`] store that can also [`Update`] and [`Remove`]
/// [`Identity`] in/from the store.
pub trait IdentitesMut: Identities + Update<Identity> + Remove<Identity> {}

impl<T> IdentitesMut for T where T: Identities + Update<Identity> + Remove<Identity> {}

/// An `Identity` store that relies on the `cache` for reads and as a
/// write-through cache.
///
/// The `store` is used for the main storage when performing a
/// write-through. It is also used for identifying which `RepoId` is
/// being used for the `cache`.
pub struct Cache<R, C> {
    store: R,
    cache: C,
}

impl<R, C> Cache<R, C> {
    pub fn new(store: R, cache: C) -> Self {
        Self { store, cache }
    }

    pub fn rid(&self) -> RepoId
    where
        R: HasRepoId,
    {
        self.store.rid()
    }
}

impl<'a, R, C> Cache<super::Identities<'a, R>, C> {
    /// Create a new [`Identity`] using the [`super::Identities`] as the
    /// main storage, and writing the update to the `cache`.
    pub fn initialize<'g, G>(
        &'a mut self,
        doc: &Doc<Verified>,
        signer: &G,
    ) -> Result<IdentityMut<'a, 'g, R, C>, super::Error>
    where
        R: ReadRepository + WriteRepository + cob::Store,
        G: Signer,
        C: Update<Identity>,
    {
        self.store.initialize(doc, &mut self.cache, signer)
    }

    /// Read the given `id` from the [`super::Identities`] store and
    /// writing it to the `cache`.
    pub fn write(&mut self, id: &ObjectId) -> Result<(), super::Error>
    where
        R: ReadRepository + cob::Store,
        C: Update<Identity>,
    {
        let identity = self.store.get(id)?;
        self.update(&self.rid(), id, &identity)
            .map_err(|e| super::Error::CacheUpdate {
                id: *id,
                err: e.into(),
            })?;
        Ok(())
    }

    /// Read all the identities from the [`super::Identities`] store and
    /// writing them to `cache`.
    ///
    /// The `callback` is used for reporting success, failures, and
    /// progress to the caller. The caller may also decide to continue
    /// or break from the process.
    pub fn write_all(
        &mut self,
        on_identity: impl Fn(
            &Result<(ObjectId, Identity), store::Error>,
            &cache::WriteAllProgress,
        ) -> ControlFlow<()>,
    ) -> Result<(), super::Error>
    where
        R: ReadRepository + cob::Store,
        C: Update<Identity>,
    {
        let identities = self.store.raw.all()?;
        let mut progress = cache::WriteAllProgress::new(identities.len());
        for identity in identities {
            progress.inc();
            match on_identity(&identity, &progress) {
                ControlFlow::Continue(()) => match identity {
                    Ok((id, identity)) => {
                        self.update(&self.rid(), &id, &identity)
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

impl<'a, R> Cache<super::Identities<'a, R>, cache::NoCache>
where
    R: ReadRepository + cob::Store,
{
    /// Get a `Cache` that does no write-through modifications and
    /// uses the [`super::Identities`] store for all reads and writes.
    pub fn no_cache(repository: &'a R) -> Result<Self, RepositoryError> {
        let store = super::Identities::open(repository)?;
        Ok(Self {
            store,
            cache: cache::NoCache,
        })
    }

    /// Get the [`IdentityMut`], identified by `id`.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<IdentityMut<'a, 'g, R, cache::NoCache>, super::Error> {
        let identity = self.store.get(id)?;

        Ok(IdentityMut {
            id: *id,
            identity,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }
}

impl<R> Cache<R, StoreReader> {
    pub fn reader(store: R, cache: StoreReader) -> Self {
        Self { store, cache }
    }
}

impl<R> Cache<R, StoreWriter> {
    pub fn open(store: R, cache: StoreWriter) -> Self {
        Self { store, cache }
    }
}

impl<'a, R> Cache<super::Identities<'a, R>, StoreWriter>
where
    R: ReadRepository + cob::Store,
{
    /// Get the [`IdentityMut`], identified by `id`, using the
    /// `StoreWriter` for retrieving the `Identity`.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<IdentityMut<'a, 'g, R, StoreWriter>, Error> {
        let identity = Identities::get(self, id)?;

        Ok(IdentityMut {
            id: *id,
            identity,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }

    /// Get the [`IdentityMut`], identified by the root identifier
    /// associated with the underlying store.
    pub fn load_mut<'g>(&'g mut self) -> Result<IdentityMut<'a, 'g, R, StoreWriter>, Error> {
        let id = ObjectId::from(self.store.raw.as_ref().identity_root()?);
        self.get_mut(&id)
    }
}

impl<R, C> cache::Update<Identity> for Cache<R, C>
where
    C: cache::Update<Identity>,
{
    type Out = <C as cache::Update<Identity>>::Out;
    type UpdateError = <C as cache::Update<Identity>>::UpdateError;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &Identity,
    ) -> Result<Self::Out, Self::UpdateError> {
        self.cache.update(rid, id, object)
    }
}

impl<R, C> cache::Remove<Identity> for Cache<R, C>
where
    C: cache::Remove<Identity>,
{
    type Out = <C as cache::Remove<Identity>>::Out;
    type RemoveError = <C as cache::Remove<Identity>>::RemoveError;

    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        self.cache.remove(id)
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

impl Update<Identity> for StoreWriter {
    type Out = bool;
    type UpdateError = UpdateError;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &Identity,
    ) -> Result<Self::Out, Self::UpdateError> {
        transaction::<_, UpdateError>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "INSERT INTO identities (id, repo, identity)
                  VALUES (?1, ?2, ?3)
                  ON CONFLICT DO UPDATE
                  SET identity =  (?3)",
            )?;

            stmt.bind((1, sql::Value::String(id.to_string())))?;
            stmt.bind((2, rid))?;
            stmt.bind((3, sql::Value::String(serde_json::to_string(&object)?)))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

impl Remove<Identity> for StoreWriter {
    type Out = bool;
    type RemoveError = sql::Error;

    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        transaction::<_, sql::Error>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "DELETE FROM identities
                  WHERE id = ?1",
            )?;

            stmt.bind((1, sql::Value::String(id.to_string())))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

pub struct NoCacheIter<'a> {
    inner: Box<dyn Iterator<Item = Result<(ObjectId, Identity), RepositoryError>> + 'a>,
}

impl<'a> Iterator for NoCacheIter<'a> {
    type Item = Result<(ObjectId, Identity), RepositoryError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, R> Identities for Cache<super::Identities<'a, R>, cache::NoCache>
where
    R: ReadRepository + cob::Store,
{
    type Error = RepositoryError;
    type Iter<'b> = NoCacheIter<'b> where Self: 'b;

    fn get(&self, id: &ObjectId) -> Result<Identity, Self::Error> {
        self.store.get(id).map_err(RepositoryError::from)
    }

    fn load(&self) -> Result<Identity, Self::Error> {
        self.store.load()
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.store
            .raw
            .all()
            .map(|inner| NoCacheIter {
                inner: Box::new(
                    inner
                        .into_iter()
                        .map(|res| res.map_err(RepositoryError::from)),
                ),
            })
            .map_err(RepositoryError::from)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
    #[error("missing identity for `{0}`")]
    Missing(RepoId),
    #[error(transparent)]
    Object(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

/// Iterator that returns a set of identities based on an SQL query.
///
/// The query is expected to return rows with columns identified by
/// the `id` and `identity` names.
pub struct IdentitiesIter<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> IdentitiesIter<'a> {
    fn parse_row(row: sql::Row) -> Result<(ObjectId, Identity), Error> {
        let id = ObjectId::from_str(row.read::<&str, _>("id"))?;
        let identity = serde_json::from_str::<Identity>(row.read::<&str, _>("identity"))?;
        Ok((id, identity))
    }
}

impl<'a> Iterator for IdentitiesIter<'a> {
    type Item = Result<(ObjectId, Identity), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(IdentitiesIter::parse_row))
    }
}

impl<R> Identities for Cache<R, StoreWriter>
where
    R: HasRepoId,
{
    type Error = Error;
    type Iter<'b> = IdentitiesIter<'b> where Self: 'b;

    fn get(&self, id: &ObjectId) -> Result<Identity, Self::Error> {
        query::get(&self.cache.db, &self.rid(), id)
    }

    fn load(&self) -> Result<Identity, Self::Error> {
        query::load(&self.cache.db, &self.rid())
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        query::list(&self.cache.db)
    }
}

impl<R> Identities for Cache<R, StoreReader>
where
    R: HasRepoId,
{
    type Error = Error;
    type Iter<'b> = IdentitiesIter<'b> where Self: 'b;

    fn get(&self, id: &ObjectId) -> Result<Identity, Self::Error> {
        query::get(&self.cache.db, &self.rid(), id)
    }

    fn load(&self) -> Result<Identity, Self::Error> {
        query::load(&self.cache.db, &self.rid())
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        query::list(&self.cache.db)
    }
}

/// Helper SQL queries for [ `Identities`] trait implementations.
mod query {
    use sqlite as sql;

    use crate::cob::identity;

    use super::*;

    pub(super) fn get(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
        id: &ObjectId,
    ) -> Result<Identity, Error> {
        let identity = sql::Value::String(id.to_string());
        let mut stmt = db.prepare(
            "SELECT identity
             FROM identities
             WHERE id = ?1 and repo = ?2",
        )?;

        stmt.bind((1, identity))?;
        stmt.bind((2, rid))?;

        match stmt.into_iter().next().transpose()? {
            None => Err(Error::NotFound((*identity::TYPENAME).clone(), *id)),
            Some(row) => {
                let identity = row.read::<&str, _>("identity");
                let identity = serde_json::from_str(identity)?;
                Ok(identity)
            }
        }
    }

    pub(super) fn load(db: &sql::ConnectionThreadSafe, rid: &RepoId) -> Result<Identity, Error> {
        let mut stmt = db.prepare(
            "SELECT identity
             FROM identities
             WHERE repo = ?1",
        )?;

        stmt.bind((1, rid))?;

        match stmt.into_iter().next().transpose()? {
            None => Err(Error::Missing(*rid)),
            Some(row) => {
                let identity = row.read::<&str, _>("identity");
                let identity = serde_json::from_str(identity)?;
                Ok(identity)
            }
        }
    }

    pub(super) fn list(db: &sql::ConnectionThreadSafe) -> Result<IdentitiesIter, Error> {
        let stmt = db.prepare(
            "SELECT id, identity
             FROM identities
            ",
        )?;
        Ok(IdentitiesIter {
            inner: stmt.into_iter(),
        })
    }
}
