use std::path::Path;
use std::{fmt, time};

use sqlite as sql;
use thiserror::Error;

/// How long to wait for the database lock to be released before failing a read.
const DB_READ_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// How long to wait for the database lock to be released before failing a write.
const DB_WRITE_TIMEOUT: time::Duration = time::Duration::from_secs(6);

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
}

/// A file-backed database storing information about the network.
pub struct Database {
    pub db: sql::Connection,
}

impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database").finish()
    }
}

impl From<sql::Connection> for Database {
    fn from(db: sql::Connection) -> Self {
        Self { db }
    }
}

impl Database {
    const SCHEMA: &'static str = include_str!("db/schema.sql");
    const PRAGMA: &'static str = "PRAGMA foreign_keys = ON";

    /// Open an address book at the given path. Creates a new address book if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open_with_flags(
            path,
            sqlite::OpenFlags::new()
                .with_create()
                .with_read_write()
                .with_full_mutex(),
        )?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::PRAGMA)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db =
            sql::Connection::open_with_flags(path, sqlite::OpenFlags::new().with_read_only())?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::PRAGMA)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open(":memory:")?;
        db.execute(Self::PRAGMA)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }
}
