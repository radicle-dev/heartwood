use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use sqlite as sql;
use thiserror::Error;

use crate::{
    clock::Timestamp,
    prelude::{Id, NodeId},
};

/// An error occuring in peer-to-peer networking code.
#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// Internal clock time overflow.
    #[error("the time overflowed")]
    TimeOverflow,
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
        let db = sql::Connection::open(path)?;
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
    /// Add a new node seeding the given id.
    fn insert(&mut self, id: Id, node: NodeId, time: Timestamp) -> Result<bool, Error>;
    /// Remove a node for the given id.
    fn remove(&mut self, id: &Id, node: &NodeId) -> Result<bool, Error>;
    /// Iterate over all entries in the routing table.
    fn entries(&self) -> Result<Box<dyn Iterator<Item = (Id, NodeId)>>, Error>;
    /// Prune entries older than the given timestamp.
    fn prune(&mut self, oldest: Timestamp) -> Result<usize, Error>;
}

impl Store for Table {
    fn get(&self, id: &Id) -> Result<HashSet<NodeId>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (node) FROM routing WHERE resource = ?")?;
        stmt.bind(1, id)?;

        let mut nodes = HashSet::new();
        for row in stmt.into_cursor() {
            nodes.insert(row?.get::<NodeId, _>("node"));
        }
        Ok(nodes)
    }

    fn get_resources(&self, node: &NodeId) -> Result<HashSet<Id>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT resource FROM routing WHERE node = ?")?;
        stmt.bind(1, node)?;

        let mut resources = HashSet::new();
        for row in stmt.into_cursor() {
            resources.insert(row?.get::<Id, _>("resource"));
        }
        Ok(resources)
    }

    fn entry(&self, id: &Id, node: &NodeId) -> Result<Option<Timestamp>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (time) FROM routing WHERE resource = ? AND node = ?")?;

        stmt.bind(1, id)?;
        stmt.bind(2, node)?;

        if let Some(Ok(row)) = stmt.into_cursor().next() {
            return Ok(Some(row.get::<i64, _>("time") as Timestamp));
        }
        Ok(None)
    }

    fn insert(&mut self, id: Id, node: NodeId, time: Timestamp) -> Result<bool, Error> {
        let time: i64 = time.try_into().map_err(|_| Error::TimeOverflow)?;
        let mut stmt = self.db.prepare(
            "INSERT INTO routing (resource, node, time)
             VALUES (?, ?, ?)
             ON CONFLICT DO NOTHING",
        )?;

        stmt.bind(1, &id)?;
        stmt.bind(2, &node)?;
        stmt.bind(3, time)?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn entries(&self) -> Result<Box<dyn Iterator<Item = (Id, NodeId)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT resource, node FROM routing ORDER BY resource")?
            .into_cursor();
        let mut entries = Vec::new();

        while let Some(Ok(row)) = stmt.next() {
            let id = row.get("resource");
            let node = row.get("node");

            entries.push((id, node));
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn remove(&mut self, id: &Id, node: &NodeId) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM routing WHERE resource = ? AND node = ?")?;

        stmt.bind(1, id)?;
        stmt.bind(2, node)?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn prune(&mut self, oldest: Timestamp) -> Result<usize, Error> {
        let oldest: i64 = oldest.try_into().map_err(|_| Error::TimeOverflow)?;
        let mut stmt = self.db.prepare("DELETE FROM routing WHERE time < ?")?;

        stmt.bind(1, oldest)?;
        stmt.next()?;

        Ok(self.db.change_count())
    }
}

#[cfg(test)]
mod test {
    use nakamoto_net::LocalTime;

    use super::*;
    use crate::test::arbitrary;

    #[test]
    fn test_insert_and_get() {
        let ids = arbitrary::set::<Id>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                assert!(db.insert(*id, *node, 0).unwrap());
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
                assert!(db.insert(*id, *node, 0).unwrap());
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
                assert!(db.insert(*id, *node, 0).unwrap());
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

        assert!(db.insert(id, node, 0).unwrap());
        assert!(!db.insert(id, node, 0).unwrap());
        assert!(!db.insert(id, node, 0).unwrap());
    }

    #[test]
    fn test_remove_redundant() {
        let id = arbitrary::gen::<Id>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = Table::open(":memory:").unwrap();

        assert!(db.insert(id, node, 0).unwrap());
        assert!(db.remove(&id, &node).unwrap());
        assert!(!db.remove(&id, &node).unwrap());
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
                let time = rng.u64(..now.as_secs());
                db.insert(*id, *node, time).unwrap();
            }
        }

        let ids = arbitrary::vec::<Id>(10);
        let nodes = arbitrary::vec::<NodeId>(10);

        for id in &ids {
            for node in &nodes {
                let time = rng.u64(now.as_secs()..i64::MAX as u64);
                db.insert(*id, *node, time).unwrap();
            }
        }

        let pruned = db.prune(now.as_secs()).unwrap();
        assert_eq!(pruned, ids.len() * nodes.len());

        for id in &ids {
            for node in &nodes {
                let t = db.entry(id, node).unwrap().unwrap();
                assert!(t >= now.as_secs());
            }
        }
    }
}
