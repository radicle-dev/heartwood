use std::collections::HashSet;
use std::path::Path;
use std::{fmt, time};

use sqlite as sql;
use thiserror::Error;

use crate::{
    prelude::Timestamp,
    prelude::{Id, NodeId},
    sql::transaction,
};

/// How long to wait for the database lock to be released before failing a read.
const DB_READ_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// How long to wait for the database lock to be released before failing a write.
const DB_WRITE_TIMEOUT: time::Duration = time::Duration::from_secs(6);

/// Result of inserting into the routing table.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InsertResult {
    /// Nothing was updated.
    NotUpdated,
    /// The entry's timestamp was updated.
    TimeUpdated,
    /// A new entry was inserted.
    SeedAdded,
}

/// An error occuring in peer-to-peer networking code.
#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// Internal unit overflow.
    #[error("the unit overflowed")]
    UnitOverflow,
}

/// Persistent file storage for a routing table.
pub struct Table {
    db: sql::Connection,
}

impl fmt::Debug for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Table(..)")
    }
}

impl Table {
    const SCHEMA: &str = include_str!("routing/schema.sql");

    /// Open a routing file store at the given path. Creates a new empty store
    /// if an existing store isn't found.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open(path)?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db =
            sql::Connection::open_with_flags(path, sqlite::OpenFlags::new().set_read_only())?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Create a new in-memory routing table.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open(":memory:")?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }
}

/// Backing store for a routing table.
pub trait Store {
    /// Get the nodes seeding the given id.
    fn get(&self, id: &Id) -> Result<HashSet<NodeId>, Error>;
    /// Get the resources seeded by the given node.
    fn get_resources(&self, node_id: &NodeId) -> Result<HashSet<Id>, Error>;
    /// Get a specific entry.
    fn entry(&self, id: &Id, node: &NodeId) -> Result<Option<Timestamp>, Error>;
    /// Checks if any entries are available.
    fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.len()? == 0)
    }
    /// Add a new node seeding the given id.
    fn insert(&mut self, id: Id, node: NodeId, time: Timestamp) -> Result<InsertResult, Error>;
    /// Remove a node for the given id.
    fn remove(&mut self, id: &Id, node: &NodeId) -> Result<bool, Error>;
    /// Iterate over all entries in the routing table.
    fn entries(&self) -> Result<Box<dyn Iterator<Item = (Id, NodeId)>>, Error>;
    /// Get the total number of routing entries.
    fn len(&self) -> Result<usize, Error>;
    /// Prune entries older than the given timestamp.
    fn prune(&mut self, oldest: Timestamp, limit: Option<usize>) -> Result<usize, Error>;
}

impl Store for Table {
    fn get(&self, id: &Id) -> Result<HashSet<NodeId>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (node) FROM routing WHERE resource = ?")?;
        stmt.bind((1, id))?;

        let mut nodes = HashSet::new();
        for row in stmt.into_iter() {
            nodes.insert(row?.read::<NodeId, _>("node"));
        }
        Ok(nodes)
    }

    fn get_resources(&self, node: &NodeId) -> Result<HashSet<Id>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT resource FROM routing WHERE node = ?")?;
        stmt.bind((1, node))?;

        let mut resources = HashSet::new();
        for row in stmt.into_iter() {
            resources.insert(row?.read::<Id, _>("resource"));
        }
        Ok(resources)
    }

    fn entry(&self, id: &Id, node: &NodeId) -> Result<Option<Timestamp>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (time) FROM routing WHERE resource = ? AND node = ?")?;

        stmt.bind((1, id))?;
        stmt.bind((2, node))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            return Ok(Some(row.read::<i64, _>("time") as Timestamp));
        }
        Ok(None)
    }

    fn insert(&mut self, id: Id, node: NodeId, time: Timestamp) -> Result<InsertResult, Error> {
        let time: i64 = time.try_into().map_err(|_| Error::UnitOverflow)?;

        transaction(&self.db, |db| {
            let mut stmt =
                db.prepare("SELECT (time) FROM routing WHERE resource = ? AND node = ?")?;

            stmt.bind((1, &id))?;
            stmt.bind((2, &node))?;

            let existed = stmt.into_iter().next().is_some();
            let mut stmt = db.prepare(
                "INSERT INTO routing (resource, node, time)
                 VALUES (?, ?, ?)
                 ON CONFLICT DO UPDATE
                 SET time = ?3
                 WHERE time < ?3",
            )?;

            stmt.bind((1, &id))?;
            stmt.bind((2, &node))?;
            stmt.bind((3, time))?;
            stmt.next()?;

            Ok(match (self.db.change_count() > 0, existed) {
                (true, true) => InsertResult::TimeUpdated,
                (true, false) => InsertResult::SeedAdded,
                (false, _) => InsertResult::NotUpdated,
            })
        })
        .map_err(Error::from)
    }

    fn entries(&self) -> Result<Box<dyn Iterator<Item = (Id, NodeId)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT resource, node FROM routing ORDER BY resource")?
            .into_iter();
        let mut entries = Vec::new();

        while let Some(Ok(row)) = stmt.next() {
            let id = row.read("resource");
            let node = row.read("node");

            entries.push((id, node));
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn remove(&mut self, id: &Id, node: &NodeId) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM routing WHERE resource = ? AND node = ?")?;

        stmt.bind((1, id))?;
        stmt.bind((2, node))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn len(&self) -> Result<usize, Error> {
        let stmt = self.db.prepare("SELECT COUNT(1) FROM routing")?;
        let count: i64 = stmt
            .into_iter()
            .next()
            .expect("COUNT will always return a single row")?
            .read(0);
        let count: usize = count.try_into().map_err(|_| Error::UnitOverflow)?;
        Ok(count)
    }

    fn prune(&mut self, oldest: Timestamp, limit: Option<usize>) -> Result<usize, Error> {
        let oldest: i64 = oldest.try_into().map_err(|_| Error::UnitOverflow)?;
        let limit: i64 = limit
            .unwrap_or(i64::MAX as usize)
            .try_into()
            .map_err(|_| Error::UnitOverflow)?;

        let mut stmt = self.db.prepare(
            "DELETE FROM routing WHERE rowid IN
            (SELECT rowid FROM routing WHERE time < ? LIMIT ?)",
        )?;
        stmt.bind((1, oldest))?;
        stmt.bind((2, limit))?;
        stmt.next()?;

        Ok(self.db.change_count())
    }
}

#[cfg(test)]
mod test {
    use localtime::LocalTime;

    use super::*;
    use crate::test::arbitrary;

    #[test]
    fn test_insert_and_get() {
        let ids = arbitrary::set::<Id>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                assert_eq!(db.insert(*id, *node, 0).unwrap(), InsertResult::SeedAdded);
            }
        }

        for id in &ids {
            let seeds = db.get(id).unwrap();
            for node in &nodes {
                assert!(seeds.contains(node));
            }
        }
    }

    #[test]
    fn test_insert_and_get_resources() {
        let ids = arbitrary::set::<Id>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                assert_eq!(db.insert(*id, *node, 0).unwrap(), InsertResult::SeedAdded);
            }
        }

        for node in &nodes {
            let projects = db.get_resources(node).unwrap();
            for id in &ids {
                assert!(projects.contains(id));
            }
        }
    }

    #[test]
    fn test_entries() {
        let ids = arbitrary::set::<Id>(6..9);
        let nodes = arbitrary::set::<NodeId>(6..9);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                assert_eq!(db.insert(*id, *node, 0).unwrap(), InsertResult::SeedAdded);
            }
        }

        let results = db.entries().unwrap().collect::<Vec<_>>();
        assert_eq!(results.len(), ids.len() * nodes.len());

        let mut results_ids = results.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        results_ids.dedup();

        assert_eq!(results_ids.len(), ids.len(), "Entries are grouped by id");
    }

    #[test]
    fn test_insert_and_remove() {
        let ids = arbitrary::set::<Id>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                db.insert(*id, *node, 0).unwrap();
            }
        }
        for id in &ids {
            for node in &nodes {
                assert!(db.remove(id, node).unwrap());
            }
        }
        for id in &ids {
            assert!(db.get(id).unwrap().is_empty());
        }
    }

    #[test]
    fn test_insert_duplicate() {
        let id = arbitrary::gen::<Id>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = Table::open(":memory:").unwrap();

        assert_eq!(db.insert(id, node, 0).unwrap(), InsertResult::SeedAdded);
        assert_eq!(db.insert(id, node, 0).unwrap(), InsertResult::NotUpdated);
        assert_eq!(db.insert(id, node, 0).unwrap(), InsertResult::NotUpdated);
    }

    #[test]
    fn test_insert_existing_updated_time() {
        let id = arbitrary::gen::<Id>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = Table::open(":memory:").unwrap();

        assert_eq!(db.insert(id, node, 0).unwrap(), InsertResult::SeedAdded);
        assert_eq!(db.insert(id, node, 1).unwrap(), InsertResult::TimeUpdated);
        assert_eq!(db.entry(&id, &node).unwrap(), Some(1));
    }

    #[test]
    fn test_remove_redundant() {
        let id = arbitrary::gen::<Id>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = Table::open(":memory:").unwrap();

        assert_eq!(db.insert(id, node, 0).unwrap(), InsertResult::SeedAdded);
        assert!(db.remove(&id, &node).unwrap());
        assert!(!db.remove(&id, &node).unwrap());
    }

    #[test]
    fn test_len() {
        let mut db = Table::open(":memory:").unwrap();
        let ids = arbitrary::vec::<Id>(10);
        let node = arbitrary::gen(1);

        for id in ids {
            db.insert(id, node, LocalTime::now().as_millis()).unwrap();
        }

        assert_eq!(10, db.len().unwrap(), "correct number of rows in table");
    }

    #[test]
    fn test_prune() {
        let rng = fastrand::Rng::new();
        let now = LocalTime::now();
        let ids = arbitrary::vec::<Id>(10);
        let nodes = arbitrary::vec::<NodeId>(10);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                let time = rng.u64(..now.as_millis());
                db.insert(*id, *node, time).unwrap();
            }
        }

        let ids = arbitrary::vec::<Id>(10);
        let nodes = arbitrary::vec::<NodeId>(10);

        for id in &ids {
            for node in &nodes {
                let time = rng.u64(now.as_millis()..i64::MAX as u64);
                db.insert(*id, *node, time).unwrap();
            }
        }

        let pruned = db.prune(now.as_millis(), None).unwrap();
        assert_eq!(pruned, ids.len() * nodes.len());

        for id in &ids {
            for node in &nodes {
                let t = db.entry(id, node).unwrap().unwrap();
                assert!(t >= now.as_millis());
            }
        }
    }
}
