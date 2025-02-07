#![allow(clippy::type_complexity)]
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::path::Path;
use std::{fmt, io, ops::Not as _, str::FromStr, time};

use sqlite as sql;
use thiserror::Error;

use crate::node::{Alias, AliasStore};
use crate::prelude::{NodeId, RepoId};

use super::{FollowPolicy, Policy, Scope, SeedPolicy, SeedingPolicy};

/// How long to wait for the database lock to be released before failing a read.
const DB_READ_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// How long to wait for the database lock to be released before failing a write.
const DB_WRITE_TIMEOUT: time::Duration = time::Duration::from_secs(6);

#[derive(Error, Debug)]
pub enum Error {
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
}

/// Read-only type witness.
pub struct Read;
/// Read-write type witness.
pub struct Write;

/// Read only config.
pub type StoreReader = Store<Read>;
/// Read-write config.
pub type StoreWriter = Store<Write>;

/// Policy configuration.
pub struct Store<T> {
    db: sql::Connection,
    _marker: PhantomData<T>,
}

impl<T> fmt::Debug for Store<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store(..)")
    }
}

impl Store<Read> {
    const SCHEMA: &'static str = include_str!("schema.sql");

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db =
            sql::Connection::open_with_flags(path, sqlite::OpenFlags::new().with_read_only())?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db,
            _marker: PhantomData,
        })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open_with_flags(
            ":memory:",
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db,
            _marker: PhantomData,
        })
    }
}

impl Store<Write> {
    const SCHEMA: &'static str = include_str!("schema.sql");

    /// Open a policy store at the given path. Creates a new store if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open(path)?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db,
            _marker: PhantomData,
        })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open(":memory:")?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db,
            _marker: PhantomData,
        })
    }

    /// Get a read-only version of this store.
    pub fn read_only(self) -> StoreReader {
        Store {
            db: self.db,
            _marker: PhantomData,
        }
    }

    /// Follow a node.
    pub fn follow(&mut self, id: &NodeId, alias: Option<&Alias>) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `following` (id, alias)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET alias = ?2 WHERE alias != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, alias.map_or("", |alias| alias.as_str())))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Seed a repository.
    pub fn seed(&mut self, id: &RepoId, scope: Scope) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `seeding` (id, scope)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET scope = ?2 WHERE scope != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, scope))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Set a node's follow policy.
    pub fn set_follow_policy(&mut self, id: &NodeId, policy: Policy) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `following` (id, policy)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET policy = ?2 WHERE policy != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, policy))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Set a repository's seeding policy.
    pub fn set_seed_policy(&mut self, id: &RepoId, policy: Policy) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `seeding` (id, policy)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET policy = ?2 WHERE policy != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, policy))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Unfollow a node.
    pub fn unfollow(&mut self, id: &NodeId) -> Result<bool, Error> {
        let mut stmt = self.db.prepare("DELETE FROM `following` WHERE id = ?")?;

        stmt.bind((1, id))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Unseed a repository.
    pub fn unseed(&mut self, id: &RepoId) -> Result<bool, Error> {
        let mut stmt = self.db.prepare("DELETE FROM `seeding` WHERE id = ?")?;

        stmt.bind((1, id))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Unblock a repository.
    pub fn unblock_rid(&mut self, id: &RepoId) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM `seeding` WHERE id = ? AND policy = 'block'")?;

        stmt.bind((1, id))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Unblock a remote.
    pub fn unblock_nid(&mut self, id: &NodeId) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM `following` WHERE id = ? AND policy = 'block'")?;

        stmt.bind((1, id))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }
}

/// `Read` methods for `Config`. This implies that a
/// `Config<Write>` can access these functions as well.
impl<T> Store<T> {
    /// Check if a node is followed.
    pub fn is_following(&self, id: &NodeId) -> Result<bool, Error> {
        Ok(matches!(
            self.follow_policy(id)?,
            Some(FollowPolicy {
                policy: Policy::Allow,
                ..
            })
        ))
    }

    /// Check if a repository is seeded.
    pub fn is_seeding(&self, id: &RepoId) -> Result<bool, Error> {
        Ok(matches!(
            self.seed_policy(id)?,
            Some(SeedPolicy { policy, .. })
            if policy.is_allow()
        ))
    }

    /// Get a node's follow policy.
    pub fn follow_policy(&self, id: &NodeId) -> Result<Option<FollowPolicy>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT alias, policy FROM `following` WHERE id = ?")?;

        stmt.bind((1, id))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let alias = row.read::<&str, _>("alias");
            let alias = alias
                .is_empty()
                .not()
                .then_some(alias.to_owned())
                .and_then(|s| Alias::from_str(&s).ok());
            let policy = row.read::<Policy, _>("policy");

            return Ok(Some(FollowPolicy {
                nid: *id,
                alias,
                policy,
            }));
        }
        Ok(None)
    }

    /// Get a repository's seeding policy.
    pub fn seed_policy(&self, id: &RepoId) -> Result<Option<SeedPolicy>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT scope, policy FROM `seeding` WHERE id = ?")?;

        stmt.bind((1, id))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let policy = match row.read::<Policy, _>("policy") {
                Policy::Allow => SeedingPolicy::Allow {
                    scope: row.read::<Scope, _>("scope"),
                },
                Policy::Block => SeedingPolicy::Block,
            };
            return Ok(Some(SeedPolicy { rid: *id, policy }));
        }
        Ok(None)
    }

    /// Get node follow policies.
    pub fn follow_policies(&self) -> Result<FollowPolicies<'_>, Error> {
        let stmt = self
            .db
            .prepare("SELECT id, alias, policy FROM `following`")?;
        Ok(FollowPolicies {
            inner: stmt.into_iter(),
        })
    }

    /// Get repository seed policies.
    pub fn seed_policies(&self) -> Result<SeedPolicies<'_>, Error> {
        let stmt = self.db.prepare("SELECT id, scope, policy FROM `seeding`")?;
        Ok(SeedPolicies {
            inner: stmt.into_iter(),
        })
    }

    pub fn nodes_by_alias<'a>(&'a self, alias: &Alias) -> Result<NodeAliasIter<'a>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT id, alias FROM `following` WHERE UPPER(alias) LIKE ?")?;
        let query = format!("%{}%", alias.as_str().to_uppercase());
        stmt.bind((1, sql::Value::String(query)))?;
        Ok(NodeAliasIter {
            inner: stmt.into_iter(),
        })
    }
}

pub struct FollowPolicies<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> Iterator for FollowPolicies<'a> {
    type Item = FollowPolicy;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        let Ok(row) = row else { return self.next() };
        let id = row.read("id");
        let alias = row.read::<&str, _>("alias").to_owned();
        let alias = alias
            .is_empty()
            .not()
            .then_some(alias.to_owned())
            .and_then(|s| Alias::from_str(&s).ok());
        let policy = row.read::<Policy, _>("policy");

        Some(FollowPolicy {
            nid: id,
            alias,
            policy,
        })
    }
}

pub struct SeedPolicies<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> Iterator for SeedPolicies<'a> {
    type Item = SeedPolicy;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        let Ok(row) = row else { return self.next() };
        let id = row.read("id");
        let policy = match row.read::<Policy, _>("policy") {
            Policy::Allow => SeedingPolicy::Allow {
                scope: row.read::<Scope, _>("scope"),
            },
            Policy::Block => SeedingPolicy::Block,
        };
        Some(SeedPolicy { rid: id, policy })
    }
}

pub struct NodeAliasIter<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> NodeAliasIter<'a> {
    fn parse_row(row: sql::Row) -> Result<(NodeId, Alias), Error> {
        let nid = row.try_read::<NodeId, _>("id")?;
        let alias = row.try_read::<Alias, _>("alias")?;
        Ok((nid, alias))
    }
}

impl<'a> Iterator for NodeAliasIter<'a> {
    type Item = Result<(NodeId, Alias), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(Self::parse_row))
    }
}

impl<T> AliasStore for Store<T> {
    /// Retrieve `alias` of given node.
    /// Calls `Self::node_policy` under the hood.
    fn alias(&self, nid: &NodeId) -> Option<Alias> {
        self.follow_policy(nid)
            .map(|node| node.and_then(|n| n.alias))
            .unwrap_or(None)
    }

    fn reverse_lookup(&self, alias: &Alias) -> BTreeMap<Alias, BTreeSet<NodeId>> {
        let Ok(iter) = self.nodes_by_alias(alias) else {
            return BTreeMap::new();
        };
        iter.flatten()
            .fold(BTreeMap::new(), |mut result, (node, alias)| {
                let nodes = result.entry(alias).or_default();
                nodes.insert(node);
                result
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use crate::{assert_matches, node};

    use super::*;
    use crate::test::arbitrary;

    #[test]
    fn test_follow_and_unfollow_node() {
        let id = arbitrary::gen::<NodeId>(1);
        let mut db = Store::open(":memory:").unwrap();
        let eve = Alias::new("eve");

        assert!(db.follow(&id, Some(&eve)).unwrap());
        assert!(db.is_following(&id).unwrap());
        assert!(!db.follow(&id, Some(&eve)).unwrap());
        assert!(db.unfollow(&id).unwrap());
        assert!(!db.is_following(&id).unwrap());
    }

    #[test]
    fn test_seed_and_unseed_repo() {
        let id = arbitrary::gen::<RepoId>(1);
        let mut db = Store::open(":memory:").unwrap();

        assert!(db.seed(&id, Scope::All).unwrap());
        assert!(db.is_seeding(&id).unwrap());
        assert!(!db.seed(&id, Scope::All).unwrap());
        assert!(db.unseed(&id).unwrap());
        assert!(!db.is_seeding(&id).unwrap());
    }

    #[test]
    fn test_node_policies() {
        let ids = arbitrary::vec::<NodeId>(3);
        let mut db = Store::open(":memory:").unwrap();

        for id in &ids {
            assert!(db.follow(id, None).unwrap());
        }
        let mut entries = db.follow_policies().unwrap();
        assert_matches!(entries.next(), Some(FollowPolicy { nid, .. }) if nid == ids[0]);
        assert_matches!(entries.next(), Some(FollowPolicy { nid, .. }) if nid == ids[1]);
        assert_matches!(entries.next(), Some(FollowPolicy { nid, .. }) if nid == ids[2]);
    }

    #[test]
    fn test_repo_policies() {
        let ids = arbitrary::vec::<RepoId>(3);
        let mut db = Store::open(":memory:").unwrap();

        for id in &ids {
            assert!(db.seed(id, Scope::All).unwrap());
        }
        let mut entries = db.seed_policies().unwrap();
        assert_matches!(entries.next(), Some(SeedPolicy { rid, .. }) if rid == ids[0]);
        assert_matches!(entries.next(), Some(SeedPolicy { rid, .. }) if rid == ids[1]);
        assert_matches!(entries.next(), Some(SeedPolicy { rid, .. }) if rid == ids[2]);
    }

    #[test]
    fn test_update_alias() {
        let id = arbitrary::gen::<NodeId>(1);
        let mut db = Store::open(":memory:").unwrap();

        assert!(db.follow(&id, Some(&Alias::new("eve"))).unwrap());
        assert_eq!(
            db.follow_policy(&id).unwrap().unwrap().alias,
            Some(Alias::from_str("eve").unwrap())
        );
        assert!(db.follow(&id, None).unwrap());
        assert_eq!(db.follow_policy(&id).unwrap().unwrap().alias, None);
        assert!(!db.follow(&id, None).unwrap());
        assert!(db.follow(&id, Some(&Alias::new("alice"))).unwrap());
        assert_eq!(
            db.follow_policy(&id).unwrap().unwrap().alias,
            Some(Alias::new("alice"))
        );
    }

    #[test]
    fn test_update_scope() {
        let id = arbitrary::gen::<RepoId>(1);
        let mut db = Store::open(":memory:").unwrap();

        assert!(db.seed(&id, Scope::All).unwrap());
        assert_eq!(
            db.seed_policy(&id).unwrap().unwrap().scope(),
            Some(Scope::All)
        );
        assert!(db.seed(&id, Scope::Followed).unwrap());
        assert_eq!(
            db.seed_policy(&id).unwrap().unwrap().scope(),
            Some(Scope::Followed)
        );
    }

    #[test]
    fn test_repo_policy() {
        let id = arbitrary::gen::<RepoId>(1);
        let mut db = Store::open(":memory:").unwrap();

        assert!(db.seed(&id, Scope::All).unwrap());
        assert!(db.seed_policy(&id).unwrap().unwrap().is_allow());
        assert!(db.set_seed_policy(&id, Policy::Block).unwrap());
        assert!(!db.seed_policy(&id).unwrap().unwrap().is_allow());
        assert_eq!(db.seed_policy(&id).unwrap().unwrap().scope(), None);
    }

    #[test]
    fn test_node_policy() {
        let id = arbitrary::gen::<NodeId>(1);
        let mut db = Store::open(":memory:").unwrap();

        assert!(db.follow(&id, None).unwrap());
        assert_eq!(
            db.follow_policy(&id).unwrap().unwrap().policy,
            Policy::Allow
        );
        assert!(db.set_follow_policy(&id, Policy::Block).unwrap());
        assert_eq!(
            db.follow_policy(&id).unwrap().unwrap().policy,
            Policy::Block
        );
    }

    #[test]
    fn test_node_aliases() {
        let mut db = Store::open(":memory:").unwrap();
        let input = node::properties::AliasInput::new();
        let (short, short_ids) = input.short();
        let (long, long_ids) = input.long();

        for id in short_ids {
            db.follow(id, Some(short)).unwrap();
        }

        for id in long_ids {
            db.follow(id, Some(long)).unwrap();
        }

        node::properties::test_reverse_lookup(&db, input)
    }
}
