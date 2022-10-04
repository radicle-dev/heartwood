use std::collections::HashSet;
use std::path::Path;

use rusqlite as sql;
use thiserror::Error;

use crate::prelude::{Id, NodeId};

/// An error occuring in peer-to-peer networking code.
#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
}

/// Persistent file storage for a routing table.
#[derive(Debug)]
pub struct Table {
    db: sql::Connection,
}

impl Table {
    const SCHEMA: &str = include_str!("routing/schema.sql");

    /// Open a routing file store at the given path. Creates a new empty store
    /// if an existing store isn't found.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let db = sql::Connection::open(path)?;
        db.execute(Self::SCHEMA, [])?;

        Ok(Self { db })
    }

    /// Create a new in-memory routing table.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open_in_memory()?;
        db.execute(Self::SCHEMA, [])?;

        Ok(Self { db })
    }
}

/// Backing store for a routing table.
pub trait Store {
    /// Get the nodes seeding the given id.
    fn get(&self, id: &Id) -> Result<HashSet<NodeId>, Error>;
    /// Add a new node seeding the given id.
    fn insert(&mut self, id: Id, node: NodeId) -> Result<bool, Error>;
    /// Remove a node for the given id.
    fn remove(&mut self, id: &Id, node: &NodeId) -> Result<bool, Error>;
    /// Iterate over all entries in the routing table.
    fn entries(&self) -> Result<Box<dyn Iterator<Item = (Id, NodeId)>>, Error>;
}

impl Store for Table {
    fn get(&self, id: &Id) -> Result<HashSet<NodeId>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (node) FROM routing WHERE resource = ?")?;
        let mut rows = stmt.query([id])?;
        let mut nodes = HashSet::new();

        while let Ok(Some(row)) = rows.next() {
            let field = row.get(0)?;
            nodes.insert(field);
        }
        Ok(nodes)
    }

    fn insert(&mut self, id: Id, node: NodeId) -> Result<bool, Error> {
        let updated = self.db.execute(
            "INSERT INTO routing (resource, node, time) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
            (id, node, 0),
        )?;

        Ok(updated > 0)
    }

    fn entries(&self) -> Result<Box<dyn Iterator<Item = (Id, NodeId)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT resource, node FROM routing ORDER BY resource")?;
        let mut rows = stmt.query([])?;
        let mut entries = Vec::new();

        while let Ok(Some(row)) = rows.next() {
            let id = row.get(0)?;
            let node = row.get(1)?;

            entries.push((id, node));
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn remove(&mut self, id: &Id, node: &NodeId) -> Result<bool, Error> {
        let deleted = self.db.execute(
            "DELETE FROM routing WHERE resource = ? AND node = ?",
            (id, node),
        )?;

        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::arbitrary;

    #[test]
    fn test_insert_and_get() {
        let ids = arbitrary::set::<Id>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                assert!(db.insert(*id, *node).unwrap());
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
    fn test_iter() {
        let ids = arbitrary::set::<Id>(6..9);
        let nodes = arbitrary::set::<NodeId>(6..9);
        let mut db = Table::open(":memory:").unwrap();

        for id in &ids {
            for node in &nodes {
                assert!(db.insert(*id, *node).unwrap());
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
                db.insert(*id, *node).unwrap();
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

        assert!(db.insert(id, node).unwrap());
        assert!(!db.insert(id, node).unwrap());
        assert!(!db.insert(id, node).unwrap());
    }

    #[test]
    fn test_remove_redundant() {
        let id = arbitrary::gen::<Id>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = Table::open(":memory:").unwrap();

        assert!(db.insert(id, node).unwrap());
        assert!(db.remove(&id, &node).unwrap());
        assert!(!db.remove(&id, &node).unwrap());
    }
}
