//! # Note on database migrations
//!
//! The `user_version` field in the database SQLite header is used to keep track of the database
//! version. It starts with `0`, which means no tables exist yet, and is incremented everytime a
//! migration is applied. In turn, migrations are named after their version numbers, so the first
//! migration is `1.sql`, the second one is `2.sql` and so on.
//!
//! The database schema is contained within the first migration. See [`version`], [`bump`] and
//! [`migrate`] for how this works.
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::{fmt, time};

use sqlite as sql;
use thiserror::Error;

use crate::sql::transaction;

/// How long to wait for the database lock to be released before failing a read.
const DB_READ_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// How long to wait for the database lock to be released before failing a write.
const DB_WRITE_TIMEOUT: time::Duration = time::Duration::from_secs(6);

/// Database migrations.
/// The first migration is the creation of the initial tables.
const MIGRATIONS: &[&str] = &[
    include_str!("db/migrations/1.sql"),
    include_str!("db/migrations/2.sql"),
    include_str!("db/migrations/3.sql"),
];

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// No rows returned in query result.
    #[error("no rows returned")]
    NoRows,
}

/// A file-backed database storing information about the network.
#[derive(Clone)]
pub struct Database {
    pub db: Arc<sql::ConnectionThreadSafe>,
}

impl Deref for Database {
    type Target = sql::ConnectionThreadSafe;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database").finish()
    }
}

impl From<sql::ConnectionThreadSafe> for Database {
    fn from(db: sql::ConnectionThreadSafe) -> Self {
        Self { db: Arc::new(db) }
    }
}

impl Database {
    const PRAGMA: &'static str = "PRAGMA foreign_keys = ON";

    /// Open a database at the given path. Creates a new database if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe(path)?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::PRAGMA)?;
        migrate(&db)?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe_with_flags(
            path,
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::PRAGMA)?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Create a new in-memory database.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open_thread_safe(":memory:")?;
        db.execute(Self::PRAGMA)?;
        migrate(&db)?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Get the database version. This is updated on schema changes.
    pub fn version(&self) -> Result<usize, Error> {
        version(&self.db)
    }

    /// Bump the database version.
    pub fn bump(&self) -> Result<usize, Error> {
        transaction(&self.db, bump)
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
pub fn bump(db: &sql::Connection) -> Result<usize, Error> {
    let old = version(db)?;
    let new = old + 1;

    db.execute(format!("PRAGMA user_version = {new}"))?;

    Ok(new as usize)
}

/// Migrate the database to the latest schema.
pub fn migrate(db: &sql::Connection) -> Result<usize, Error> {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_version() {
        let n = MIGRATIONS.len();
        let db = Database::memory().unwrap();
        assert_eq!(db.version().unwrap(), n);

        let v = db.bump().unwrap();
        assert_eq!(v, n + 1);
        assert_eq!(db.version().unwrap(), n + 1);

        let v = db.bump().unwrap();
        assert_eq!(v, n + 2);
        assert_eq!(db.version().unwrap(), n + 2);
    }
}
