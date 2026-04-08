//! # Note on database migrations
//!
//! The `user_version` field in the database SQLite header is used to keep track of the database
//! version. It starts with `0`, which means no tables exist yet, and is incremented every time a
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

use crate::node::{
    Address, Alias, Features, KnownAddress, NodeId, PROTOCOL_VERSION, Timestamp, UserAgent, address,
};
use crate::sql::transaction;

pub mod config;
pub mod sqlite_ext;

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
    include_str!("db/migrations/4.sql"),
    include_str!("db/migrations/5.sql"),
    include_str!("db/migrations/6.sql"),
    include_str!("db/migrations/7.sql"),
    include_str!("db/migrations/8.sql"),
];

#[derive(Error, Debug)]
pub enum Error {
    /// Initialization error.
    #[error("error initializing the database: {0}")]
    Init(#[from] address::store::Error),
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
    /// does not exist.
    pub fn open<P: AsRef<Path>>(path: P, config: config::Config) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe(path)?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::PRAGMA)?;
        migrate(&db)?;

        Self { db: Arc::new(db) }.configure(config)
    }

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P, config: config::Config) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe_with_flags(
            path,
            sql::OpenFlags::new().with_read_only(),
        )?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::PRAGMA)?;

        Self { db: Arc::new(db) }.configure(config)
    }

    /// Set `journal_mode` pragma.
    pub fn journal_mode(self, mode: sqlite_ext::JournalMode) -> Result<Self, Error> {
        self.db.execute(format!("PRAGMA journal_mode = {mode};"))?;
        Ok(self)
    }

    /// Set the `synchronous` pragma.
    pub fn synchronous(self, synchronous: sqlite_ext::Synchronous) -> Result<Self, Error> {
        self.db
            .execute(format!("PRAGMA synchronous = {synchronous};"))?;
        Ok(self)
    }

    /// Initialize by adding our local node to the database.
    pub fn init<'a>(
        mut self,
        node: &NodeId,
        features: Features,
        alias: &Alias,
        agent: &UserAgent,
        timestamp: Timestamp,
        addrs: impl IntoIterator<Item = &'a Address>,
    ) -> Result<Self, Error> {
        address::Store::insert(
            &mut self,
            node,
            PROTOCOL_VERSION,
            features,
            alias,
            0,
            agent,
            timestamp,
            addrs
                .into_iter()
                .map(|a| KnownAddress::new(a.clone(), address::Source::Imported)),
        )?;

        Ok(self)
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

    #[cfg(test)]
    fn memory_up_to_migration(n: usize) -> Result<Self, Error> {
        if n == 0 {
            panic!("Migration number 'n' must be larger than 0");
        } else if n > MIGRATIONS.len() {
            panic!(
                "Migration number {n} exceeds the number of migrations {}",
                MIGRATIONS.len()
            );
        }
        let db = sql::Connection::open_thread_safe(":memory:")?;
        db.execute(Self::PRAGMA)?;
        {
            let mut version = version(&db)?;
            for (i, migration) in MIGRATIONS.iter().enumerate().take(n) {
                if i >= version {
                    transaction(&db, |db| {
                        db.execute(migration)?;
                        version = bump(db)?;

                        Ok::<_, Error>(())
                    })?;
                }
            }
        }
        Ok(Self { db: Arc::new(db) })
    }

    fn configure(self, config: config::Config) -> Result<Self, Error> {
        self.journal_mode(config.sqlite.pragma.journal_mode)?
            .synchronous(config.sqlite.pragma.synchronous)
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
#[allow(clippy::unwrap_used)]
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

    mod migration_8 {
        use super::*;

        const NODE1: &str = "node1";
        const NODE2: &str = "node2";

        fn db_before_migration() -> Database {
            let db = Database::memory_up_to_migration(7).unwrap();
            db.execute(
                "INSERT INTO nodes (id, features, alias, timestamp)
             VALUES ('node1', 0, 'alias', 0)",
            )
            .unwrap();
            db.execute(
                "INSERT INTO nodes (id, features, alias, timestamp)
         VALUES ('node2', 0, 'alias2', 0)",
            )
            .unwrap();
            db
        }

        fn run_migration(db: &Database) {
            db.execute(MIGRATIONS[7]).unwrap();
        }

        fn address_count(db: &Database, node: &str, address_type: &str, value: &str) -> i64 {
            db.prepare(format!(
                "SELECT COUNT(*) FROM addresses
             WHERE node = '{node}' AND type = '{address_type}' AND value = '{value}'"
            ))
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .unwrap()
            .read::<i64, _>(0)
        }

        fn insert_address(db: &Database, node: &str, address_type: &str, value: &str) {
            db.execute(format!(
                "INSERT INTO addresses (node, type, value, source, timestamp)
             VALUES ('{node}', '{address_type}', '{value}', 'peer', 0)"
            ))
            .unwrap();
        }

        #[test]
        fn ipv6_formatted_dns_address_is_retyped_to_ipv6() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "dns", "[::1]:8776");

            run_migration(&db);

            assert_eq!(address_count(&db, NODE1, "ipv6", "[::1]:8776"), 1);
            assert_eq!(address_count(&db, NODE1, "dns", "[::1]:8776"), 0);
        }

        #[test]
        fn ipv6_formatted_dns_address_is_deleted_when_correct_ipv6_row_already_exists() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "ipv6", "[2001:db8::1]:8776");
            insert_address(&db, NODE1, "dns", "[2001:db8::1]:8776");

            run_migration(&db);

            assert_eq!(
                address_count(&db, NODE1, "ipv6", "[2001:db8::1]:8776"),
                1,
                "existing ipv6 row survives"
            );
            assert_eq!(
                address_count(&db, NODE1, "dns", "[2001:db8::1]:8776"),
                0,
                "stale dns row is removed"
            );
        }

        #[test]
        fn plain_dns_hostname_without_brackets_is_unaffected() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "dns", "example.com:8776");

            run_migration(&db);

            assert_eq!(address_count(&db, NODE1, "dns", "example.com:8776"), 1);
        }

        #[test]
        fn dns_address_with_bracket_not_at_start_is_unaffected() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "dns", "foo[::1]:8776");

            run_migration(&db);

            assert_eq!(address_count(&db, NODE1, "dns", "foo[::1]:8776"), 1);
        }

        // The `Address` type always contains a port so this case should never
        // be hit, but recording it here for posterity.
        #[test]
        fn dns_address_starting_with_bracket_but_missing_closing_bracket_colon_is_unaffected() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "dns", "[::1]");

            run_migration(&db);

            assert_eq!(address_count(&db, NODE1, "dns", "[::1]"), 1);
        }

        #[test]
        fn ipv4_address_is_unaffected() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "ipv4", "192.168.1.1:8776");

            run_migration(&db);

            assert_eq!(address_count(&db, NODE1, "ipv4", "192.168.1.1:8776"), 1);
        }

        #[test]
        fn retype_preserves_address_metadata() {
            let db = db_before_migration();
            db.execute(
        "INSERT INTO addresses (node, type, value, source, timestamp, last_attempt, last_success, banned)
         VALUES ('node1', 'dns', '[::1]:8776', 'peer', 0, 1000, 2000, 1)"
    ).unwrap();

            run_migration(&db);

            let row = db
                .prepare(
                    "SELECT last_attempt, last_success, banned FROM addresses
         WHERE node = 'node1' AND type = 'ipv6' AND value = '[::1]:8776'",
                )
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
                .unwrap();

            assert_eq!(row.read::<i64, _>("last_attempt"), 1000);
            assert_eq!(row.read::<i64, _>("last_success"), 2000);
            assert_eq!(row.read::<i64, _>("banned"), 1);
        }

        #[test]
        fn migration_applies_to_all_nodes() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "dns", "[::1]:8776");
            insert_address(&db, NODE2, "dns", "[::1]:8776");

            run_migration(&db);

            assert_eq!(
                address_count(&db, NODE1, "ipv6", "[::1]:8776"),
                1,
                "node1 address retyped"
            );
            assert_eq!(
                address_count(&db, NODE2, "ipv6", "[::1]:8776"),
                1,
                "node1 address retyped"
            );
        }

        #[test]
        fn all_ipv6_formatted_dns_addresses_are_retyped() {
            let db = db_before_migration();
            insert_address(&db, NODE1, "dns", "[::1]:8776");
            insert_address(&db, NODE1, "dns", "[2001:db8::1]:8776");
            insert_address(&db, NODE1, "dns", "[fe80::1]:8776");

            run_migration(&db);

            for value in ["[::1]:8776", "[2001:db8::1]:8776", "[fe80::1]:8776"] {
                assert_eq!(
                    address_count(&db, NODE1, "ipv6", value),
                    1,
                    "{value} should be ipv6"
                );
                assert_eq!(
                    address_count(&db, NODE1, "dns", value),
                    0,
                    "{value} dns row should be gone"
                );
            }
        }
    }
}
