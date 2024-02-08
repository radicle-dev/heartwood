use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;
use std::time;

use radicle_cob::ObjectId;
use sqlite as sql;
use thiserror::Error;

use crate::prelude::RepoId;
use crate::sql::transaction;

/// File suffix for storing the COBs database file.
pub const COBS_DB_FILE: &str = "cache.db";

/// How long to wait for the database lock to be released before failing a read.
const DB_READ_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// How long to wait for the database lock to be released before failing a write.
const DB_WRITE_TIMEOUT: time::Duration = time::Duration::from_secs(6);

/// Database migrations.
/// The first migration is the creation of the initial tables.
const MIGRATIONS: &[&str] = &[include_str!("cache/migrations/1.sql")];

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// No rows returned in query result.
    #[error("no rows returned")]
    NoRows,
}

/// Read and write to the store.
pub type StoreWriter = Store<Write>;
/// Write to the store.
pub type StoreReader = Store<Read>;

/// Read-only type witness.
#[derive(Clone)]
pub struct Read;
/// Read-write type witness.
#[derive(Clone)]
pub struct Write;

/// A file-backed database storing information about the network.
#[derive(Clone)]
pub struct Store<T> {
    pub(super) db: Arc<sql::ConnectionThreadSafe>,
    marker: PhantomData<T>,
}

impl<T> fmt::Debug for Store<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database").finish()
    }
}

impl Store<Read> {
    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe_with_flags(
            path,
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }

    /// Create a new in-memory database.
    pub fn memory() -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe_with_flags(
            ":memory:",
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }
}

impl Store<Write> {
    /// Open a database at the given path. Creates a new database if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe(path)?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        migrate(&db)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }

    /// Create a new in-memory database.
    pub fn memory() -> Result<Self, Error> {
        let db = Arc::new(sql::Connection::open_thread_safe(":memory:")?);
        migrate(&db)?;

        Ok(Self {
            db,
            marker: PhantomData,
        })
    }

    /// Turn this handle into a read-only handle.
    pub fn read_only(self) -> Store<Read> {
        Store {
            db: self.db,
            marker: PhantomData,
        }
    }

    /// Perform a raw query on the database handle.
    pub fn raw_query<T, E, F>(&self, query: F) -> Result<T, E>
    where
        F: FnOnce(&sql::Connection) -> Result<T, E>,
        E: From<sql::Error>,
    {
        transaction(&self.db, query)
    }
}

impl<T> Store<T> {
    /// Get the database version. This is updated on schema changes.
    pub fn version(&self) -> Result<usize, Error> {
        version(&self.db)
    }
}

/// Get the `user_version` value from the database header.
pub fn version(db: &sql::Connection) -> Result<usize, Error> {
    let version = db
        .prepare("PRAGMA user_version")?
        .into_iter()
        .next()
        .ok_or(Error::NoRows)??
        .read::<i64, _>(0);

    Ok(version as usize)
}

/// Bump the `user_version` value.
fn bump(db: &sql::Connection) -> Result<usize, Error> {
    let old = version(db)?;
    let new = old + 1;

    db.execute(format!("PRAGMA user_version = {new}"))?;

    Ok(new as usize)
}

/// Migrate the database to the latest schema.
fn migrate(db: &sql::Connection) -> Result<usize, Error> {
    let mut version = version(db)?;
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        if i >= version {
            transaction(db, |db| {
                db.execute(migration)?;
                version = bump(db)?;

                Ok::<_, Error>(())
            })?;
        }
    }
    Ok(version)
}

/// Update a COB object in the cache.
pub trait Update<T> {
    /// The output type, if any, for a successful update.
    type Out;
    type UpdateError: std::error::Error + Send + Sync + 'static;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &T,
    ) -> Result<Self::Out, Self::UpdateError>;
}

/// Remove a COB object in the cache.
pub trait Remove<T> {
    /// The output type, if any, for a successful removal.
    type Out;
    type RemoveError: std::error::Error + Send + Sync + 'static;

    /// Delete an object in the COB cache.
    ///
    /// This assumes that the `id` is unique across repositories.
    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError>;
}

/// An in-memory cache for storing COB objects.
///
/// The intention is for this to be used in tests that expect cache
/// reads.
#[derive(Clone, Debug)]
pub struct InMemory<T> {
    inner: HashMap<RepoId, HashMap<ObjectId, T>>,
}

impl<T> Default for InMemory<T> {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl<T> Update<T> for InMemory<T>
where
    T: Clone,
{
    type Out = Option<T>;
    type UpdateError = Infallible;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &T,
    ) -> Result<Self::Out, Self::UpdateError> {
        let objects = self.inner.entry(*rid).or_default();
        Ok(objects.insert(*id, object.clone()))
    }
}

/// The `/dev/null` of caches.
///
/// It will ignore any updates, and successfully return on each call
/// of [`Update::update`].
///
/// The intention is for this to be used in tests that do not expect
/// any cache reads.
pub struct NoCache;

impl<T> Update<T> for NoCache {
    type Out = ();
    type UpdateError = Infallible;

    fn update(
        &mut self,
        _rid: &RepoId,
        _id: &ObjectId,
        _object: &T,
    ) -> Result<Self::Out, Self::UpdateError> {
        Ok(())
    }
}

impl<T> Remove<T> for NoCache {
    type Out = ();
    type RemoveError = Infallible;

    fn remove(&mut self, _id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        Ok(())
    }
}

/// Track the progress of cache writes when transferring the
/// repository COBs to their respective caches.
///
/// See [`crate::cob::issue::Cache::write_all`] and
/// [`crate::cob::patch::Cache::write_all`].
pub struct WriteAllProgress {
    total: usize,
    seen: usize,
}

impl WriteAllProgress {
    /// Create a new progress tracker with the given `total` amount.
    pub fn new(total: usize) -> Self {
        Self { total, seen: 0 }
    }

    /// Increment the [`WriteAllProgress::seen`] progress.
    pub fn inc(&mut self) {
        self.seen += 1;
    }

    /// Return the `total` amount.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Return the `seen` amount.
    pub fn seen(&self) -> usize {
        self.seen
    }

    /// Return the percentage of the progress made.
    ///
    /// # Panics
    ///
    /// If the `total` provided is `0`.
    pub fn percentage(&self) -> f32 {
        (self.seen as f32 / self.total as f32) * 100.0
    }
}
