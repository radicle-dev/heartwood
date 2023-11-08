use std::path::Path;
use std::str::FromStr;
use std::{fmt, io};

use localtime::LocalTime;
use sqlite as sql;
use thiserror::Error;

use crate::node;
use crate::node::address::{KnownAddress, Source};
use crate::node::{Address, Alias, AliasError, AliasStore, NodeId};
use crate::prelude::Timestamp;
use crate::sql::transaction;

use super::types;
use super::AddressType;

#[derive(Error, Debug)]
pub enum Error {
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("alias error: {0}")]
    InvalidAlias(#[from] AliasError),
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
}

/// A file-backed address book.
pub struct Book {
    db: sql::Connection,
}

impl fmt::Debug for Book {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Book(..)")
    }
}

impl From<sql::Connection> for Book {
    fn from(db: sql::Connection) -> Self {
        Self { db }
    }
}

impl Book {
    const SCHEMA: &str = include_str!("schema.sql");

    /// Open an address book at the given path. Creates a new address book if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let db = sql::Connection::open_with_flags(
            path,
            sqlite::OpenFlags::new()
                .with_create()
                .with_read_write()
                .with_full_mutex(),
        )?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let db = sql::Connection::open_with_flags(path, sqlite::OpenFlags::new().with_read_only())?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open(":memory:")?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }
}

impl Store for Book {
    fn get(&self, node: &NodeId) -> Result<Option<types::Node>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT features, alias, pow, timestamp FROM nodes WHERE id = ?")?;

        stmt.bind((1, node))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let features = row.read::<node::Features, _>("features");
            let alias = Alias::from_str(row.read::<&str, _>("alias"))?;
            let timestamp = row.read::<i64, _>("timestamp") as Timestamp;
            let pow = row.read::<i64, _>("pow") as u32;
            let mut addrs = Vec::new();

            let mut stmt = self
                .db
                .prepare("SELECT type, value, source FROM addresses WHERE node = ?")?;
            stmt.bind((1, node))?;

            for row in stmt.into_iter() {
                let row = row?;
                let _typ = row.read::<AddressType, _>("type");
                let addr = row.read::<Address, _>("value");
                let source = row.read::<Source, _>("source");

                addrs.push(KnownAddress {
                    addr,
                    source,
                    last_success: None,
                    last_attempt: None,
                });
            }

            Ok(Some(types::Node {
                features,
                alias,
                pow,
                timestamp,
                addrs,
            }))
        } else {
            Ok(None)
        }
    }

    fn len(&self) -> Result<usize, Error> {
        let row = self
            .db
            .prepare("SELECT COUNT(*) FROM addresses")?
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        let count = row.read::<i64, _>(0) as usize;

        Ok(count)
    }

    fn insert(
        &mut self,
        node: &NodeId,
        features: node::Features,
        alias: Alias,
        pow: u32,
        timestamp: Timestamp,
        addrs: impl IntoIterator<Item = KnownAddress>,
    ) -> Result<bool, Error> {
        transaction(&self.db, move |db| {
            let mut stmt = db.prepare(
                "INSERT INTO nodes (id, features, alias, pow, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT DO UPDATE
                 SET features = ?2, alias = ?3, pow = ?4, timestamp = ?5
                 WHERE timestamp < ?5",
            )?;

            stmt.bind((1, node))?;
            stmt.bind((2, features))?;
            stmt.bind((3, sql::Value::String(alias.into())))?;
            stmt.bind((4, pow as i64))?;
            stmt.bind((5, timestamp as i64))?;
            stmt.next()?;

            for addr in addrs {
                let mut stmt = db.prepare(
                    "INSERT INTO addresses (node, type, value, source, timestamp)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT DO UPDATE
                     SET timestamp = ?5
                     WHERE timestamp < ?5",
                )?;
                stmt.bind((1, node))?;
                stmt.bind((2, AddressType::from(&addr.addr)))?;
                stmt.bind((3, &addr.addr))?;
                stmt.bind((4, addr.source))?;
                stmt.bind((5, timestamp as i64))?;
                stmt.next()?;
            }
            Ok(db.change_count() > 0)
        })
        .map_err(Error::from)
    }

    fn remove(&mut self, node: &NodeId) -> Result<bool, Error> {
        transaction(&self.db, move |db| {
            db.prepare("DELETE FROM nodes WHERE id = ?")?
                .into_iter()
                .bind(&[node][..])?
                .next();

            db.prepare("DELETE FROM addresses WHERE node = ?")?
                .into_iter()
                .bind(&[node][..])?
                .next();

            Ok(db.change_count() > 0)
        })
        .map_err(Error::from)
    }

    fn entries(&self) -> Result<Box<dyn Iterator<Item = (NodeId, KnownAddress)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT node, type, value, source, last_success, last_attempt FROM addresses ORDER BY node")?
            .into_iter();
        let mut entries = Vec::new();

        while let Some(Ok(row)) = stmt.next() {
            let node = row.read::<NodeId, _>("node");
            let _typ = row.read::<AddressType, _>("type");
            let addr = row.read::<Address, _>("value");
            let source = row.read::<Source, _>("source");
            let last_success = row.read::<Option<i64>, _>("last_success");
            let last_attempt = row.read::<Option<i64>, _>("last_attempt");
            let last_success = last_success.map(|t| LocalTime::from_millis(t as u128));
            let last_attempt = last_attempt.map(|t| LocalTime::from_millis(t as u128));

            entries.push((
                node,
                KnownAddress {
                    addr,
                    source,
                    last_success,
                    last_attempt,
                },
            ));
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn attempted(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error> {
        let mut stmt = self.db.prepare(
            "UPDATE `addresses`
             SET last_attempt = ?1
             WHERE node = ?2
             AND type = ?3
             AND value = ?4",
        )?;

        stmt.bind((1, time as i64))?;
        stmt.bind((2, nid))?;
        stmt.bind((3, AddressType::from(addr)))?;
        stmt.bind((4, addr))?;
        stmt.next()?;

        Ok(())
    }

    fn connected(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error> {
        let mut stmt = self.db.prepare(
            "UPDATE `addresses`
             SET last_success = ?1
             WHERE node = ?2
             AND type = ?3
             AND value = ?4",
        )?;

        stmt.bind((1, time as i64))?;
        stmt.bind((2, nid))?;
        stmt.bind((3, AddressType::from(addr)))?;
        stmt.bind((4, addr))?;
        stmt.next()?;

        Ok(())
    }
}

impl AliasStore for Book {
    /// Retrieve `alias` of given node.
    /// Calls `Self::get` under the hood.
    fn alias(&self, nid: &NodeId) -> Option<Alias> {
        self.get(nid)
            .map(|node| node.map(|n| n.alias))
            .unwrap_or(None)
    }
}

/// Address store.
///
/// Used to store node addresses and metadata.
pub trait Store {
    /// Get a known peer address.
    fn get(&self, id: &NodeId) -> Result<Option<types::Node>, Error>;
    /// Insert a node with associated addresses into the store.
    ///
    /// Returns `true` if the node or addresses were updated, and `false` otherwise.
    fn insert(
        &mut self,
        node: &NodeId,
        features: node::Features,
        alias: Alias,
        pow: u32,
        timestamp: Timestamp,
        addrs: impl IntoIterator<Item = KnownAddress>,
    ) -> Result<bool, Error>;
    /// Remove an address from the store.
    fn remove(&mut self, id: &NodeId) -> Result<bool, Error>;
    /// Returns the number of addresses.
    fn len(&self) -> Result<usize, Error>;
    /// Returns true if there are no addresses.
    fn is_empty(&self) -> Result<bool, Error> {
        self.len().map(|l| l == 0)
    }
    /// Get the address entries in the store.
    fn entries(&self) -> Result<Box<dyn Iterator<Item = (NodeId, KnownAddress)>>, Error>;
    /// Mark a node as attempted at a certain time.
    fn attempted(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error>;
    /// Mark a node as successfully connected at a certain time.
    fn connected(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error>;
}

impl TryFrom<&sql::Value> for Source {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        let err = sql::Error {
            code: None,
            message: Some("sql: invalid source".to_owned()),
        };
        match value {
            sql::Value::String(s) => match s.as_str() {
                "bootstrap" => Ok(Source::Bootstrap),
                "peer" => Ok(Source::Peer),
                "imported" => Ok(Source::Imported),
                _ => Err(err),
            },
            _ => Err(err),
        }
    }
}

impl sql::BindableWithIndex for Source {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        match self {
            Self::Bootstrap => "bootstrap".bind(stmt, i),
            Self::Peer => "peer".bind(stmt, i),
            Self::Imported => "imported".bind(stmt, i),
        }
    }
}

impl TryFrom<&sql::Value> for AddressType {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        let err = sql::Error {
            code: None,
            message: Some("sql: invalid address type".to_owned()),
        };
        match value {
            sql::Value::String(s) => match s.as_str() {
                "ipv4" => Ok(AddressType::Ipv4),
                "ipv6" => Ok(AddressType::Ipv6),
                "dns" => Ok(AddressType::Dns),
                "onion" => Ok(AddressType::Onion),
                _ => Err(err),
            },
            _ => Err(err),
        }
    }
}

impl sql::BindableWithIndex for AddressType {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        match self {
            Self::Ipv4 => "ipv4".bind(stmt, i),
            Self::Ipv6 => "ipv6".bind(stmt, i),
            Self::Dns => "dns".bind(stmt, i),
            Self::Onion => "onion".bind(stmt, i),
        }
    }
}

#[cfg(test)]
mod test {
    use std::net;

    use super::*;
    use crate::test::arbitrary;
    use localtime::LocalTime;

    #[test]
    fn test_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cache");
        let cache = Book::open(path).unwrap();

        assert!(cache.is_empty().unwrap());
    }

    #[test]
    fn test_get_none() {
        let alice = arbitrary::gen::<NodeId>(1);
        let cache = Book::memory().unwrap();
        let result = cache.get(&alice).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_remove_nothing() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Book::memory().unwrap();
        let removed = cache.remove(&alice).unwrap();

        assert!(!removed);
    }

    #[test]
    fn test_alias() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Book::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = LocalTime::now().as_millis();

        cache
            .insert(&alice, features, Alias::new("alice"), 16, timestamp, [])
            .unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.alias.as_ref(), "alice");

        cache
            .insert(&alice, features, Alias::new("bob"), 16, timestamp + 1, [])
            .unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.alias.as_ref(), "bob");
    }

    #[test]
    fn test_insert_and_get() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Book::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = LocalTime::now().as_millis();

        let ka = KnownAddress {
            addr: net::SocketAddr::from(([4, 4, 4, 4], 8776)).into(),
            source: Source::Peer,
            last_success: None,
            last_attempt: None,
        };
        let inserted = cache
            .insert(
                &alice,
                features,
                Alias::new("alice"),
                16,
                timestamp,
                [ka.clone()],
            )
            .unwrap();
        assert!(inserted);

        let node = cache.get(&alice).unwrap().unwrap();

        assert_eq!(node.features, features);
        assert_eq!(node.pow, 16);
        assert_eq!(node.timestamp, timestamp);
        assert_eq!(node.alias.as_ref(), "alice");
        assert_eq!(node.addrs, vec![ka]);
    }

    #[test]
    fn test_insert_duplicate() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Book::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = LocalTime::now().as_millis();
        let alias = Alias::new("alice");

        let ka = KnownAddress {
            addr: net::SocketAddr::from(([4, 4, 4, 4], 8776)).into(),
            source: Source::Peer,
            last_success: None,
            last_attempt: None,
        };
        let inserted = cache
            .insert(&alice, features, alias.clone(), 0, timestamp, [ka.clone()])
            .unwrap();
        assert!(inserted);

        let inserted = cache
            .insert(&alice, features, alias, 0, timestamp, [ka])
            .unwrap();
        assert!(!inserted);

        assert_eq!(cache.len().unwrap(), 1);
    }

    #[test]
    fn test_insert_and_update() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Book::memory().unwrap();
        let timestamp = LocalTime::now().as_millis();
        let features = node::Features::SEED;
        let alias1 = Alias::new("alice");
        let alias2 = Alias::new("~alice~");
        let ka = KnownAddress {
            addr: net::SocketAddr::from(([4, 4, 4, 4], 8776)).into(),
            source: Source::Peer,
            last_success: None,
            last_attempt: None,
        };

        let updated = cache
            .insert(&alice, features, alias1, 0, timestamp, [ka.clone()])
            .unwrap();
        assert!(updated);

        let updated = cache
            .insert(&alice, features, alias2.clone(), 0, timestamp, [])
            .unwrap();
        assert!(!updated, "Can't update using the same timestamp");

        let updated = cache
            .insert(&alice, features, alias2.clone(), 0, timestamp - 1, [])
            .unwrap();
        assert!(!updated, "Can't update using a smaller timestamp");

        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.alias.as_ref(), "alice");
        assert_eq!(node.timestamp, timestamp);
        assert_eq!(node.pow, 0);

        let updated = cache
            .insert(&alice, features, alias2.clone(), 0, timestamp + 1, [])
            .unwrap();
        assert!(updated, "Can update with a larger timestamp");

        let updated = cache
            .insert(&alice, node::Features::NONE, alias2, 1, timestamp + 2, [])
            .unwrap();
        assert!(updated);

        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.features, node::Features::NONE);
        assert_eq!(node.alias.as_ref(), "~alice~");
        assert_eq!(node.timestamp, timestamp + 2);
        assert_eq!(node.pow, 1);
        assert_eq!(node.addrs, vec![ka]);
    }

    #[test]
    fn test_insert_and_remove() {
        let alice = arbitrary::gen::<NodeId>(1);
        let bob = arbitrary::gen::<NodeId>(1);
        let mut cache = Book::memory().unwrap();
        let timestamp = LocalTime::now().as_millis();
        let features = node::Features::SEED;
        let alice_alias = Alias::new("alice");
        let bob_alias = Alias::new("bob");

        for addr in [
            ([4, 4, 4, 4], 8776),
            ([7, 7, 7, 7], 8776),
            ([9, 9, 9, 9], 8776),
        ] {
            let ka = KnownAddress {
                addr: net::SocketAddr::from(addr).into(),
                source: Source::Peer,
                last_success: None,
                last_attempt: None,
            };
            cache
                .insert(
                    &alice,
                    features,
                    alice_alias.clone(),
                    0,
                    timestamp,
                    [ka.clone()],
                )
                .unwrap();
            cache
                .insert(&bob, features, bob_alias.clone(), 0, timestamp, [ka])
                .unwrap();
        }
        assert_eq!(cache.len().unwrap(), 6);

        let removed = cache.remove(&alice).unwrap();
        assert!(removed);
        assert_eq!(cache.len().unwrap(), 3);

        let removed = cache.remove(&bob).unwrap();
        assert!(removed);
        assert_eq!(cache.len().unwrap(), 0);
    }

    #[test]
    fn test_entries() {
        let ids = arbitrary::vec::<NodeId>(16);
        let mut rng = fastrand::Rng::new();
        let mut cache = Book::memory().unwrap();
        let mut expected = Vec::new();
        let timestamp = LocalTime::now().as_millis();
        let features = node::Features::SEED;
        let alias = Alias::new("alice");

        for id in ids {
            let ip = rng.u32(..);
            let addr = net::SocketAddr::from((net::Ipv4Addr::from(ip), rng.u16(..)));
            let ka = KnownAddress {
                addr: addr.into(),
                source: Source::Bootstrap,
                // TODO: Test times as well.
                last_success: None,
                last_attempt: None,
            };
            expected.push((id, ka.clone()));
            cache
                .insert(&id, features, alias.clone(), 0, timestamp, [ka])
                .unwrap();
        }

        let mut actual = cache.entries().unwrap().collect::<Vec<_>>();

        actual.sort_by_key(|(i, _)| *i);
        expected.sort_by_key(|(i, _)| *i);

        assert_eq!(cache.len().unwrap(), actual.len());
        assert_eq!(actual, expected);
    }
}
