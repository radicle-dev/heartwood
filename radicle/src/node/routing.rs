use std::collections::HashSet;

use sqlite as sql;
use thiserror::Error;

use crate::node::Database;
use crate::{
    prelude::Timestamp,
    prelude::{NodeId, RepoId},
    sql::transaction,
};

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

/// Backing store for a routing table.
pub trait Store {
    /// Get the nodes seeding the given id.
    fn get(&self, id: &RepoId) -> Result<HashSet<NodeId>, Error>;
    /// Get the inventory seeded by the given node.
    fn get_inventory(&self, node_id: &NodeId) -> Result<HashSet<RepoId>, Error>;
    /// Get a specific entry.
    fn entry(&self, id: &RepoId, node: &NodeId) -> Result<Option<Timestamp>, Error>;
    /// Checks if any entries are available.
    fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.len()? == 0)
    }
    /// Add a new node seeding the given id.
    fn add_inventory<'a>(
        &mut self,
        ids: impl IntoIterator<Item = &'a RepoId>,
        node: NodeId,
        time: Timestamp,
    ) -> Result<Vec<(RepoId, InsertResult)>, Error>;
    /// Remove an inventory from the given node.
    fn remove_inventory(&mut self, id: &RepoId, node: &NodeId) -> Result<bool, Error>;
    /// Remove multiple inventories from the given node.
    fn remove_inventories<'a>(
        &mut self,
        ids: impl IntoIterator<Item = &'a RepoId>,
        node: &NodeId,
    ) -> Result<(), Error>;
    /// Iterate over all entries in the routing table.
    fn entries(&self) -> Result<Box<dyn Iterator<Item = (RepoId, NodeId)>>, Error>;
    /// Get the total number of routing entries.
    fn len(&self) -> Result<usize, Error>;
    /// Prune entries older than the given timestamp.
    fn prune(
        &mut self,
        oldest: Timestamp,
        limit: Option<usize>,
        ignore: &NodeId,
    ) -> Result<usize, Error>;
    /// Count the number of routes for a specific repo RID.
    fn count(&self, id: &RepoId) -> Result<usize, Error>;
}

impl Store for Database {
    fn get(&self, id: &RepoId) -> Result<HashSet<NodeId>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (node) FROM routing WHERE repo = ?")?;
        stmt.bind((1, id))?;

        let mut nodes = HashSet::new();
        for row in stmt.into_iter() {
            nodes.insert(row?.read::<NodeId, _>("node"));
        }
        Ok(nodes)
    }

    fn get_inventory(&self, node: &NodeId) -> Result<HashSet<RepoId>, Error> {
        let mut stmt = self.db.prepare("SELECT repo FROM routing WHERE node = ?")?;
        stmt.bind((1, node))?;

        let mut inventory = HashSet::new();
        for row in stmt.into_iter() {
            inventory.insert(row?.read::<RepoId, _>("repo"));
        }
        Ok(inventory)
    }

    fn entry(&self, id: &RepoId, node: &NodeId) -> Result<Option<Timestamp>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT (timestamp) FROM routing WHERE repo = ? AND node = ?")?;

        stmt.bind((1, id))?;
        stmt.bind((2, node))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            return Ok(Some(row.read::<Timestamp, _>("timestamp")));
        }
        Ok(None)
    }

    fn add_inventory<'a>(
        &mut self,
        ids: impl IntoIterator<Item = &'a RepoId>,
        node: NodeId,
        time: Timestamp,
    ) -> Result<Vec<(RepoId, InsertResult)>, Error> {
        let mut results = Vec::new();
        let mut select_stmt = self
            .db
            .prepare("SELECT (timestamp) FROM routing WHERE repo = ? AND node = ?")?;
        let mut insert_stmt = self.db.prepare(
            "INSERT INTO routing (repo, node, timestamp)
             VALUES (?, ?, ?)
             ON CONFLICT DO UPDATE
             SET timestamp = ?3
             WHERE timestamp < ?3",
        )?;
        transaction(&self.db, |_| {
            for id in ids.into_iter() {
                select_stmt.bind((1, id))?;
                select_stmt.bind((2, &node))?;

                let existed = select_stmt.iter().next().is_some();
                select_stmt.reset()?;

                insert_stmt.bind((1, id))?;
                insert_stmt.bind((2, &node))?;
                insert_stmt.bind((3, &time))?;
                insert_stmt.next()?;
                insert_stmt.reset()?;

                let result = match (self.db.change_count() > 0, existed) {
                    (true, true) => InsertResult::TimeUpdated,
                    (true, false) => InsertResult::SeedAdded,
                    (false, _) => InsertResult::NotUpdated,
                };
                results.push((*id, result));
            }
            Ok::<_, Error>(results)
        })
    }

    fn entries(&self) -> Result<Box<dyn Iterator<Item = (RepoId, NodeId)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT repo, node FROM routing ORDER BY repo")?
            .into_iter();
        let mut entries = Vec::new();

        while let Some(Ok(row)) = stmt.next() {
            let id = row.read("repo");
            let node = row.read("node");

            entries.push((id, node));
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn remove_inventory(&mut self, id: &RepoId, node: &NodeId) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM routing WHERE repo = ? AND node = ?")?;

        stmt.bind((1, id))?;
        stmt.bind((2, node))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn remove_inventories<'a>(
        &mut self,
        rids: impl IntoIterator<Item = &'a RepoId>,
        nid: &NodeId,
    ) -> Result<(), Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM routing WHERE repo = ? AND node = ?")?;

        transaction(&self.db, |_| {
            for rid in rids.into_iter() {
                stmt.bind((1, rid))?;
                stmt.bind((2, nid))?;

                stmt.iter().next();
                stmt.reset()?;
            }
            Ok::<_, Error>(())
        })
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

    fn prune(
        &mut self,
        oldest: Timestamp,
        limit: Option<usize>,
        ignore: &NodeId,
    ) -> Result<usize, Error> {
        let limit: i64 = limit
            .unwrap_or(i64::MAX as usize)
            .try_into()
            .map_err(|_| Error::UnitOverflow)?;

        let mut stmt = self.db.prepare(
            "DELETE FROM routing
             WHERE node <> ?1 AND rowid IN
             (SELECT rowid FROM routing WHERE timestamp < ?2 ORDER BY timestamp LIMIT ?3)",
        )?;
        stmt.bind((1, ignore))?;
        stmt.bind((2, &oldest))?;
        stmt.bind((3, limit))?;
        stmt.next()?;

        Ok(self.db.change_count())
    }

    fn count(&self, id: &RepoId) -> Result<usize, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT COUNT(*) FROM routing WHERE repo = ?")?;

        stmt.bind((1, id))?;

        let count: i64 = stmt
            .into_iter()
            .next()
            .expect("COUNT will always return a single row")?
            .read(0);

        let count: usize = count.try_into().map_err(|_| Error::UnitOverflow)?;

        Ok(count)
    }
}

#[cfg(test)]
mod test {
    use localtime::LocalTime;

    use super::*;
    use crate::test::arbitrary;

    fn database(path: &str) -> Database {
        let db = Database::open(path).unwrap();

        // We don't want to test foreign key constraints here.
        db.db.execute("PRAGMA foreign_keys = OFF").unwrap();
        db
    }

    #[test]
    fn test_insert_and_get() {
        let ids = arbitrary::set::<RepoId>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = database(":memory:");

        for node in &nodes {
            assert_eq!(
                db.add_inventory(&ids, *node, Timestamp::EPOCH).unwrap(),
                ids.iter()
                    .map(|id| (*id, InsertResult::SeedAdded))
                    .collect::<Vec<_>>()
            );
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
        let ids = arbitrary::set::<RepoId>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = database(":memory:");

        for node in &nodes {
            db.add_inventory(&ids, *node, Timestamp::EPOCH).unwrap();
        }

        for node in &nodes {
            let projects = db.get_inventory(node).unwrap();
            for id in &ids {
                assert!(projects.contains(id));
            }
        }
    }

    #[test]
    fn test_entries() {
        let ids = arbitrary::set::<RepoId>(6..9);
        let nodes = arbitrary::set::<NodeId>(6..9);
        let mut db = database(":memory:");

        for node in &nodes {
            assert!(db
                .add_inventory(&ids, *node, Timestamp::EPOCH)
                .unwrap()
                .iter()
                .all(|(_, r)| *r == InsertResult::SeedAdded));
        }

        let results = db.entries().unwrap().collect::<Vec<_>>();
        assert_eq!(results.len(), ids.len() * nodes.len());

        let mut results_ids = results.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        results_ids.dedup();

        assert_eq!(results_ids.len(), ids.len(), "Entries are grouped by id");
    }

    #[test]
    fn test_insert_and_remove() {
        let ids = arbitrary::set::<RepoId>(5..10);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = database(":memory:");

        for node in &nodes {
            db.add_inventory(&ids, *node, Timestamp::EPOCH).unwrap();
        }
        for id in &ids {
            for node in &nodes {
                assert!(db.remove_inventory(id, node).unwrap());
            }
        }
        for id in &ids {
            assert!(db.get(id).unwrap().is_empty());
        }
    }

    #[test]
    fn test_insert_duplicate() {
        let id = arbitrary::gen::<RepoId>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = database(":memory:");

        assert_eq!(
            db.add_inventory([&id], node, Timestamp::EPOCH).unwrap(),
            vec![(id, InsertResult::SeedAdded)]
        );
        assert_eq!(
            db.add_inventory([&id], node, Timestamp::EPOCH).unwrap(),
            vec![(id, InsertResult::NotUpdated)]
        );
        assert_eq!(
            db.add_inventory([&id], node, Timestamp::EPOCH).unwrap(),
            vec![(id, InsertResult::NotUpdated)]
        );
    }

    #[test]
    fn test_insert_existing_updated_time() {
        let id = arbitrary::gen::<RepoId>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = database(":memory:");

        assert_eq!(
            db.add_inventory([&id], node, Timestamp::EPOCH).unwrap(),
            vec![(id, InsertResult::SeedAdded)]
        );
        assert_eq!(
            db.add_inventory([&id], node, Timestamp::try_from(1u64).unwrap())
                .unwrap(),
            vec![(id, InsertResult::TimeUpdated)]
        );
        assert_eq!(
            db.entry(&id, &node).unwrap(),
            Some(Timestamp::try_from(1u64).unwrap())
        );
    }

    #[test]
    fn test_update_existing_multi() {
        let id1 = arbitrary::gen::<RepoId>(1);
        let id2 = arbitrary::gen::<RepoId>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = database(":memory:");

        assert_eq!(
            db.add_inventory([&id1], node, Timestamp::EPOCH).unwrap(),
            vec![(id1, InsertResult::SeedAdded)]
        );
        assert_eq!(
            db.add_inventory([&id1, &id2], node, Timestamp::EPOCH)
                .unwrap(),
            vec![
                (id1, InsertResult::NotUpdated),
                (id2, InsertResult::SeedAdded)
            ]
        );
        assert_eq!(
            db.add_inventory([&id1, &id2], node, Timestamp::try_from(1u64).unwrap())
                .unwrap(),
            vec![
                (id1, InsertResult::TimeUpdated),
                (id2, InsertResult::TimeUpdated)
            ]
        );
    }

    #[test]
    fn test_remove_redundant() {
        let id = arbitrary::gen::<RepoId>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = database(":memory:");

        assert_eq!(
            db.add_inventory([&id], node, Timestamp::EPOCH).unwrap(),
            vec![(id, InsertResult::SeedAdded)]
        );
        assert!(db.remove_inventory(&id, &node).unwrap());
        assert!(!db.remove_inventory(&id, &node).unwrap());
    }

    #[test]
    fn test_remove_many() {
        let id1 = arbitrary::gen::<RepoId>(1);
        let id2 = arbitrary::gen::<RepoId>(1);
        let id3 = arbitrary::gen::<RepoId>(1);
        let node = arbitrary::gen::<NodeId>(1);
        let mut db = database(":memory:");

        db.add_inventory([&id1, &id2, &id3], node, Timestamp::EPOCH)
            .unwrap();
        assert_eq!(db.len().unwrap(), 3);

        db.remove_inventories([&id1, &id3], &node).unwrap();
        assert_eq!(db.len().unwrap(), 1);
    }

    #[test]
    fn test_len() {
        let mut db = database(":memory:");
        let ids = arbitrary::vec::<RepoId>(10);
        let node = arbitrary::gen(1);

        db.add_inventory(&ids, node, LocalTime::now().into())
            .unwrap();

        assert_eq!(10, db.len().unwrap(), "correct number of rows in table");
    }

    #[test]
    fn test_prune() {
        let mut rng = fastrand::Rng::new();
        let now = LocalTime::now();
        let ids = arbitrary::vec::<RepoId>(10);
        let nodes = arbitrary::vec::<NodeId>(10);
        let mut db = database(":memory:");

        for node in &nodes {
            let time = rng.u64(..now.as_millis());
            db.add_inventory(&ids, *node, Timestamp::try_from(time).unwrap())
                .unwrap();
        }

        let ids = arbitrary::vec::<RepoId>(10);
        let nodes = arbitrary::vec::<NodeId>(10);

        for node in &nodes {
            let time = rng.u64(now.as_millis()..i64::MAX as u64);
            db.add_inventory(&ids, *node, Timestamp::try_from(time).unwrap())
                .unwrap();
        }

        let pruned = db.prune(now.into(), None, &arbitrary::gen(1)).unwrap();
        assert_eq!(pruned, ids.len() * nodes.len());

        for id in &ids {
            for node in &nodes {
                let t = db.entry(id, node).unwrap().unwrap();
                assert!(*t >= *Timestamp::from(now));
            }
        }
    }

    #[test]
    fn test_count() {
        let id = arbitrary::gen::<RepoId>(1);
        let nodes = arbitrary::set::<NodeId>(5..10);
        let mut db = database(":memory:");

        for node in &nodes {
            db.add_inventory([&id], *node, Timestamp::EPOCH).unwrap();
        }
        assert_eq!(db.count(&id).unwrap(), nodes.len());
    }
}
