mod migrations;

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
const MIGRATIONS: &[Migration] = &[
    Migration::Sql(include_str!("cache/migrations/1.sql")),
    Migration::Native(migrations::_2::run),
];

/// Function signature for native migrations.
type MigrateFn = fn(&sql::Connection, &Progress, &mut dyn MigrateCallback) -> Result<usize, Error>;

/// A database migration.
enum Migration {
    /// Migration written in SQL.
    Sql(&'static str),
    /// Migration function written in Rust.
    Native(MigrateFn),
}

/// Progress of a database migration.
#[derive(Debug)]
pub struct MigrateProgress<'a> {
    /// Progress in the list of migrations.
    pub migration: &'a Progress,
    /// Progress within each individual migration.
    pub rows: &'a Progress,
}

impl MigrateProgress<'_> {
    /// If we're done with the migration.
    pub fn is_done(&self) -> bool {
        self.migration.current() == self.migration.total()
            && self.rows.current() == self.rows.total()
    }
}

/// Something that can process migration progress.
pub trait MigrateCallback {
    /// A migration has progressed.
    fn progress(&mut self, progress: MigrateProgress<'_>);
}

impl<F> MigrateCallback for F
where
    F: Fn(MigrateProgress),
{
    fn progress(&mut self, progress: MigrateProgress) {
        (self)(progress)
    }
}

/// Migration functions that implement [`MigrateCallback`].
pub mod migrate {
    use super::*;

    /// Log progress via installed logger at "info" level.
    pub fn log(progress: MigrateProgress<'_>) {
        log::trace!(
            target: "db",
            "Migration {}/{} in progress.. ({}%)",
            progress.migration.current(),
            progress.migration.total(),
            progress.rows.percentage()
        );
    }

    /// Ignore progress, just migrate.
    pub fn ignore(_progress: MigrateProgress<'_>) {}
}

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// Malformed JSON schema, eg. missing fields or wrong field types.
    #[error("malformed JSON schema")]
    MalformedJsonSchema,
    /// Malformed JSON data, ie. not valid JSON.
    #[error("malformed JSON data: {0}")]
    MalformedJson(serde_json::Error),
    /// No rows returned in query result.
    #[error("no rows returned")]
    NoRows,
    /// Schema is out of date, migrations need to be run.
    #[error("collaborative objects database is out of date")]
    OutOfDate,
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

/// A file-backed database storing materialized COBs.
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

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }

    /// Create a new in-memory database.
    pub fn memory() -> Result<Self, Error> {
        let db = Arc::new(sql::Connection::open_thread_safe(":memory:")?);

        Ok(Self {
            db,
            marker: PhantomData,
        })
    }

    /// Builder method that migrates the database.
    pub fn with_migrations<M: MigrateCallback>(mut self, callback: M) -> Result<Self, Error> {
        self.migrate(callback).map(|_| self)
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

    /// Migrate this database to the latest version.
    /// Returns the verison migrated to.
    pub fn migrate<M: MigrateCallback>(&mut self, callback: M) -> Result<usize, Error> {
        self.migrate_to(MIGRATIONS.len(), callback)
    }

    /// Migrate this database to the given target version.
    /// Returns the verison migrated to.
    pub fn migrate_to<M: MigrateCallback>(
        &mut self,
        target: usize,
        mut callback: M,
    ) -> Result<usize, Error> {
        let db = &self.db;
        let mut version = version(db)?;
        let total = MIGRATIONS.len();

        for (i, migration) in MIGRATIONS.iter().enumerate().take(target).skip(version) {
            let current = i + 1;

            transaction(db, |db| {
                match migration {
                    Migration::Sql(query) => {
                        db.execute(query)?;
                        callback.progress(MigrateProgress {
                            migration: &Progress { total, current },
                            rows: &Progress::done(1),
                        });
                    }
                    Migration::Native(migrate) => {
                        migrate(db, &Progress { total, current }, &mut callback)?;
                    }
                }
                version = bump(db)?;

                Ok::<_, Error>(())
            })?;
        }
        Ok(version)
    }
}

impl<T> Store<T> {
    /// Get the database version. This is updated on schema changes.
    pub fn version(&self) -> Result<usize, Error> {
        version(&self.db)
    }

    /// Check if the database version is out of date, ie. we need to migrate.
    pub fn check_version(&self) -> Result<(), Error> {
        if version(&self.db)? < MIGRATIONS.len() {
            return Err(Error::OutOfDate);
        }
        Ok(())
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
    /// Delete all entries from a repo.
    fn remove_all(&mut self, rid: &RepoId) -> Result<Self::Out, Self::RemoveError>;
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

    fn remove_all(&mut self, _rid: &RepoId) -> Result<Self::Out, Self::RemoveError> {
        Ok(())
    }
}

/// Track the progress of cache writes when transferring the
/// repository COBs to their respective caches.
///
/// See [`crate::cob::issue::Cache::write_all`] and
/// [`crate::cob::patch::Cache::write_all`].
#[derive(Debug)]
pub struct Progress {
    current: usize,
    total: usize,
}

impl Progress {
    /// Create a new progress tracker with the given `total` amount.
    pub fn new(total: usize) -> Self {
        Self { current: 0, total }
    }

    /// Create a new progress tracker that is "done".
    pub fn done(total: usize) -> Self {
        Self {
            current: total,
            total,
        }
    }

    /// Increment the [`Progress::current`] progress.
    pub fn inc(&mut self) {
        self.current += 1;
    }

    /// Return the `total` amount.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Return the `current` amount.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Return the percentage of the progress made.
    pub fn percentage(&self) -> f32 {
        if self.total == 0 {
            100.
        } else {
            (self.current as f32 / self.total as f32) * 100.0
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::assert_matches;

    #[test]
    fn test_check_version() {
        let mut db = StoreWriter::memory().unwrap();
        assert_matches!(db.check_version(), Err(Error::OutOfDate));

        db.migrate(migrate::ignore).unwrap();
        assert_matches!(db.check_version(), Ok(()));
    }

    #[test]
    fn test_migrate_to() {
        let mut db = StoreWriter::memory().unwrap();
        assert_eq!(db.version().unwrap(), 0);

        assert_eq!(db.migrate_to(1, migrate::ignore).unwrap(), 1); // 0 -> 1
        assert_eq!(db.version().unwrap(), 1);

        assert_eq!(db.migrate_to(2, migrate::ignore).unwrap(), 2); // 1 -> 2
        assert_eq!(db.version().unwrap(), 2);

        assert_eq!(db.migrate_to(1, migrate::ignore).unwrap(), 2); // No-op.
        assert_eq!(db.version().unwrap(), 2);

        assert_eq!(db.migrate_to(99, migrate::ignore).unwrap(), 2); // No-op.
        assert_eq!(db.version().unwrap(), 2);
    }
}
