use std::net::IpAddr;
use std::num::TryFromIntError;
use std::str::FromStr;

use localtime::LocalTime;
use sqlite as sql;
use thiserror::Error;

use crate::node;
use crate::node::address::{AddressType, KnownAddress, Node, Source};
use crate::node::UserAgent;
use crate::node::{Address, Alias, AliasError, AliasStore, Database, NodeId, Penalty, Severity};
use crate::prelude::Timestamp;
use crate::sql::transaction;

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    #[error("alias error: {0}")]
    InvalidAlias(#[from] AliasError),
    #[error("integer conversion error: {0}")]
    TryFromInt(#[from] TryFromIntError),
    /// No rows returned in query result.
    #[error("no rows returned")]
    NoRows,
}

/// An entry returned by the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressEntry {
    /// Node ID.
    pub node: NodeId,
    /// Node protocol version.
    pub version: u8,
    /// Node penalty.
    pub penalty: Penalty,
    /// Node address.
    pub address: KnownAddress,
}

/// Address store.
///
/// Used to store node addresses and metadata.
pub trait Store {
    /// Get the information we have about a node.
    fn get(&self, id: &NodeId) -> Result<Option<Node>, Error>;
    /// Get the addresses of a node.
    fn addresses_of(&self, node: &NodeId) -> Result<Vec<KnownAddress>, Error>;
    /// Insert a node with associated addresses into the store.
    ///
    /// Returns `true` if the node or addresses were updated, and `false` otherwise.
    fn insert(
        &mut self,
        node: &NodeId,
        version: u8,
        features: node::Features,
        alias: Alias,
        pow: u32,
        agent: &UserAgent,
        timestamp: Timestamp,
        addrs: impl IntoIterator<Item = KnownAddress>,
    ) -> Result<bool, Error>;
    /// Remove a node from the store.
    fn remove(&mut self, id: &NodeId) -> Result<bool, Error>;
    /// Returns the number of addresses.
    fn len(&self) -> Result<usize, Error>;
    /// Return the number of nodes.
    fn nodes(&self) -> Result<usize, Error>;
    /// Returns true if there are no addresses.
    fn is_empty(&self) -> Result<bool, Error> {
        self.len().map(|l| l == 0)
    }
    /// Check if an address is banned. Also returns `true` if the node this address belongs
    /// to is banned.
    fn is_addr_banned(&self, addr: &Address) -> Result<bool, Error>;
    /// Check if an IP is banned.
    fn is_ip_banned(&self, ip: IpAddr) -> Result<bool, Error>;
    /// Get the address entries in the store.
    fn entries(&self) -> Result<Box<dyn Iterator<Item = AddressEntry>>, Error>;
    /// Mark a node as attempted at a certain time.
    fn attempted(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error>;
    /// Mark a node as successfully connected at a certain time.
    fn connected(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error>;
    /// Record a node IP address and connection time.
    fn record_ip(&self, nid: &NodeId, ip: IpAddr, time: Timestamp) -> Result<(), Error>;
    /// Mark a node as disconnected.
    fn disconnected(
        &mut self,
        nid: &NodeId,
        addr: &Address,
        severity: Severity,
    ) -> Result<(), Error>;
}

impl Store for Database {
    fn get(&self, node: &NodeId) -> Result<Option<Node>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT version, features, alias, pow, penalty, banned, agent, timestamp
             FROM nodes
             WHERE id = ?",
        )?;
        stmt.bind((1, node))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let version = row.read::<i64, _>("version").try_into()?;
            let features = row.read::<node::Features, _>("features");
            let alias = Alias::from_str(row.read::<&str, _>("alias"))?;
            let timestamp = row.read::<Timestamp, _>("timestamp");
            let pow = row.read::<i64, _>("pow") as u32;
            let agent = row.read::<UserAgent, _>("agent");
            let penalty = row.read::<i64, _>("penalty").min(u8::MAX as i64);
            let penalty = Penalty(penalty as u8);
            let banned = row.read::<i64, _>("banned").is_positive();
            let addrs = self.addresses_of(node)?;

            Ok(Some(Node {
                version,
                features,
                alias,
                pow,
                agent,
                timestamp,
                penalty,
                addrs,
                banned,
            }))
        } else {
            Ok(None)
        }
    }

    fn is_addr_banned(&self, addr: &Address) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "SELECT a.banned, n.banned
             FROM addresses AS a
             JOIN nodes AS n ON a.node = n.id
             WHERE value = ?1 AND type = ?2",
        )?;
        stmt.bind((1, addr))?;
        stmt.bind((2, AddressType::from(addr)))?;

        if let Some(row) = stmt.into_iter().next() {
            let row = row?;
            let addr_banned = row.read::<i64, _>(0).is_positive();
            let node_banned = row.read::<i64, _>(1).is_positive();

            Ok(node_banned || addr_banned)
        } else {
            Ok(false)
        }
    }

    fn is_ip_banned(&self, ip: IpAddr) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "SELECT banned
             FROM ips
             WHERE ip = ?1 AND banned > 0",
        )?;
        stmt.bind((1, ip.to_string().as_str()))?;

        Ok(stmt.into_iter().next().is_some())
    }

    fn addresses_of(&self, node: &NodeId) -> Result<Vec<KnownAddress>, Error> {
        let mut addrs = Vec::new();
        let mut stmt = self.db.prepare(
            "SELECT type, value, source, last_attempt, last_success, banned FROM addresses WHERE node = ?",
        )?;
        stmt.bind((1, node))?;

        for row in stmt.into_iter() {
            let row = row?;
            let _typ = row.read::<AddressType, _>("type");
            let addr = row.read::<Address, _>("value");
            let source = row.read::<Source, _>("source");
            let last_attempt = row
                .read::<Option<i64>, _>("last_attempt")
                .map(|t| LocalTime::from_millis(t as u128));
            let last_success = row
                .read::<Option<i64>, _>("last_success")
                .map(|t| LocalTime::from_millis(t as u128));
            let banned = row.read::<i64, _>("banned").is_positive();

            addrs.push(KnownAddress {
                addr,
                source,
                last_success,
                last_attempt,
                banned,
            });
        }
        Ok(addrs)
    }

    fn len(&self) -> Result<usize, Error> {
        let row = self
            .db
            .prepare("SELECT COUNT(*) FROM addresses")?
            .into_iter()
            .next()
            .ok_or(Error::NoRows)??;
        let count = row.read::<i64, _>(0) as usize;

        Ok(count)
    }

    fn nodes(&self) -> Result<usize, Error> {
        let row = self
            .db
            .prepare("SELECT COUNT(*) FROM nodes")?
            .into_iter()
            .next()
            .ok_or(Error::NoRows)??;
        let count = row.read::<i64, _>(0) as usize;

        Ok(count)
    }

    fn insert(
        &mut self,
        node: &NodeId,
        version: u8,
        features: node::Features,
        alias: Alias,
        pow: u32,
        agent: &UserAgent,
        timestamp: Timestamp,
        addrs: impl IntoIterator<Item = KnownAddress>,
    ) -> Result<bool, Error> {
        transaction(&self.db, move |db| {
            let mut stmt = db.prepare(
                "INSERT INTO nodes (id, version, features, alias, pow, agent, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT DO UPDATE
                 SET version = ?2, features = ?3, alias = ?4, pow = ?5, agent = ?6, timestamp = ?7
                 WHERE timestamp < ?7",
            )?;

            stmt.bind((1, node))?;
            stmt.bind((2, version as i64))?;
            stmt.bind((3, features))?;
            stmt.bind((4, sql::Value::String(alias.into())))?;
            stmt.bind((5, pow as i64))?;
            stmt.bind((6, agent.as_str()))?;
            stmt.bind((7, &timestamp))?;
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
                stmt.bind((5, &timestamp))?;
                stmt.next()?;
            }
            Ok::<_, Error>(db.change_count() > 0)
        })
    }

    fn remove(&mut self, node: &NodeId) -> Result<bool, Error> {
        let mut stmt = self.db.prepare("DELETE FROM nodes WHERE id = ?1")?;

        stmt.bind((1, node))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn entries(&self) -> Result<Box<dyn Iterator<Item = AddressEntry>>, Error> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT a.node, a.type, a.value, a.source, a.last_success, a.last_attempt, a.banned, n.version, n.penalty
                 FROM addresses AS a
                 JOIN nodes AS n ON a.node = n.id
                 ORDER BY n.penalty ASC, n.id ASC",
            )?
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
            let version = row.read::<i64, _>("version").try_into()?;
            let banned = row.read::<i64, _>("banned").is_positive();
            let penalty = row.read::<i64, _>("penalty");
            let penalty = Penalty(penalty as u8); // Clamped at `u8::MAX`.

            entries.push(AddressEntry {
                node,
                version,
                penalty,
                address: KnownAddress {
                    addr,
                    source,
                    last_success,
                    last_attempt,
                    banned,
                },
            });
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

        stmt.bind((1, &time))?;
        stmt.bind((2, nid))?;
        stmt.bind((3, AddressType::from(addr)))?;
        stmt.bind((4, addr))?;
        stmt.next()?;

        Ok(())
    }

    fn connected(&self, nid: &NodeId, addr: &Address, time: Timestamp) -> Result<(), Error> {
        transaction(&self.db, |db| {
            let mut stmt = db.prepare(
                "UPDATE `addresses`
                 SET last_success = ?1
                 WHERE node = ?2
                 AND type = ?3
                 AND value = ?4",
            )?;

            stmt.bind((1, &time))?;
            stmt.bind((2, nid))?;
            stmt.bind((3, AddressType::from(addr)))?;
            stmt.bind((4, addr))?;
            stmt.next()?;

            // Reduce penalty by half on successful connect.
            let mut stmt = db.prepare("UPDATE `nodes` SET penalty = penalty / 2 WHERE id = ?1")?;

            stmt.bind((1, nid))?;
            stmt.next()?;

            Ok(())
        })
    }

    fn record_ip(&self, nid: &NodeId, ip: IpAddr, time: Timestamp) -> Result<(), Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO ips (ip, node, last_attempt)
             VALUES (?1, ?2, ?3)
             ON CONFLICT DO UPDATE
             SET last_attempt = ?3
             WHERE last_attempt < ?3",
        )?;
        stmt.bind((1, ip.to_string().as_str()))?;
        stmt.bind((2, nid))?;
        stmt.bind((3, &time))?;
        stmt.next()?;

        Ok(())
    }

    fn disconnected(
        &mut self,
        nid: &NodeId,
        addr: &Address,
        severity: Severity,
    ) -> Result<(), Error> {
        transaction(&self.db, |db| {
            let mut stmt = self.db.prepare(
                "UPDATE `nodes`
                 SET penalty = penalty + ?2
                 WHERE id = ?1",
            )?;
            stmt.bind((1, nid))?;
            stmt.bind((2, severity as i64))?;
            stmt.next()?;

            // If the ban threshold is reached, we ban the node and its addresses.
            let node = self.get(nid)?.ok_or(Error::NoRows)?;
            if node.penalty.is_ban_threshold_reached() {
                let mut stmt = db.prepare("UPDATE `nodes` SET banned = 1 WHERE id = ?1")?;
                stmt.bind((1, nid))?;
                stmt.next()?;

                let mut stmt = db.prepare("UPDATE `addresses` SET banned = 1 WHERE value = ?1")?;
                stmt.bind((1, addr))?;
                stmt.next()?;

                let mut stmt = db.prepare("UPDATE `ips` SET banned = 1 WHERE node = ?1")?;
                stmt.bind((1, nid))?;
                stmt.next()?;
            }
            Ok(())
        })
    }
}

impl<T> AliasStore for T
where
    T: Store,
{
    /// Retrieve `alias` of given node.
    /// Calls `Self::get` under the hood.
    fn alias(&self, nid: &NodeId) -> Option<Alias> {
        self.get(nid)
            .map(|node| node.map(|n| n.alias))
            .unwrap_or(None)
    }
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
    use cyphernet::addr::NetAddr;
    use localtime::LocalTime;

    #[test]
    fn test_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cache");
        let cache = Database::open(path).unwrap();

        assert!(cache.is_empty().unwrap());
    }

    #[test]
    fn test_get_none() {
        let alice = arbitrary::gen::<NodeId>(1);
        let cache = Database::memory().unwrap();
        let result = cache.get(&alice).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_remove_nothing() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Database::memory().unwrap();
        let removed = cache.remove(&alice).unwrap();

        assert!(!removed);
    }

    #[test]
    fn test_alias() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Database::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = Timestamp::from(LocalTime::now());
        let ua = UserAgent::default();

        cache
            .insert(
                &alice,
                1,
                features,
                Alias::new("alice"),
                16,
                &ua,
                timestamp,
                [],
            )
            .unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.alias.as_ref(), "alice");

        cache
            .insert(
                &alice,
                1,
                features,
                Alias::new("bob"),
                16,
                &ua,
                timestamp + 1,
                [],
            )
            .unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.alias.as_ref(), "bob");
    }

    #[test]
    fn test_insert_and_get() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Database::memory().unwrap();
        let version = 2;
        let features = node::Features::SEED;
        let timestamp = LocalTime::now().into();
        let ua = UserAgent::default();

        let ka = KnownAddress {
            addr: net::SocketAddr::from(([4, 4, 4, 4], 8776)).into(),
            source: Source::Peer,
            last_success: None,
            last_attempt: None,
            banned: false,
        };
        let inserted = cache
            .insert(
                &alice,
                version,
                features,
                Alias::new("alice"),
                16,
                &ua,
                timestamp,
                [ka.clone()],
            )
            .unwrap();
        assert!(inserted);

        let node = cache.get(&alice).unwrap().unwrap();

        assert_eq!(node.version, version);
        assert_eq!(node.features, features);
        assert_eq!(node.pow, 16);
        assert_eq!(node.timestamp, timestamp);
        assert_eq!(node.alias.as_ref(), "alice");
        assert_eq!(node.addrs, vec![ka]);
    }

    #[test]
    fn test_insert_duplicate() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Database::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = LocalTime::now().into();
        let alias = Alias::new("alice");
        let ua = UserAgent::default();

        let ka = KnownAddress {
            addr: net::SocketAddr::from(([4, 4, 4, 4], 8776)).into(),
            source: Source::Peer,
            last_success: None,
            last_attempt: None,
            banned: false,
        };
        let inserted = cache
            .insert(
                &alice,
                1,
                features,
                alias.clone(),
                0,
                &ua,
                timestamp,
                [ka.clone()],
            )
            .unwrap();
        assert!(inserted);

        let inserted = cache
            .insert(&alice, 1, features, alias, 0, &ua, timestamp, [ka])
            .unwrap();
        assert!(!inserted);

        assert_eq!(cache.len().unwrap(), 1);
    }

    #[test]
    fn test_insert_and_update() {
        let alice = arbitrary::gen::<NodeId>(1);
        let mut cache = Database::memory().unwrap();
        let timestamp = LocalTime::now().into();
        let features = node::Features::SEED;
        let ua1 = UserAgent::default();
        let ua2 = UserAgent::default();
        let alias1 = Alias::new("alice");
        let alias2 = Alias::new("~alice~");
        let ka = KnownAddress {
            addr: net::SocketAddr::from(([4, 4, 4, 4], 8776)).into(),
            source: Source::Peer,
            last_success: None,
            last_attempt: None,
            banned: false,
        };

        let updated = cache
            .insert(
                &alice,
                1,
                features,
                alias1,
                0,
                &ua1,
                timestamp,
                [ka.clone()],
            )
            .unwrap();
        assert!(updated);

        let updated = cache
            .insert(&alice, 1, features, alias2.clone(), 0, &ua1, timestamp, [])
            .unwrap();
        assert!(!updated, "Can't update using the same timestamp");

        let updated = cache
            .insert(
                &alice,
                1,
                features,
                alias2.clone(),
                0,
                &ua1,
                timestamp - 1,
                [],
            )
            .unwrap();
        assert!(!updated, "Can't update using a smaller timestamp");

        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.alias.as_ref(), "alice");
        assert_eq!(node.timestamp, timestamp);
        assert_eq!(node.pow, 0);

        let updated = cache
            .insert(
                &alice,
                1,
                features,
                alias2.clone(),
                0,
                &ua2,
                timestamp + 1,
                [],
            )
            .unwrap();
        assert!(updated, "Can update with a larger timestamp");

        let updated = cache
            .insert(
                &alice,
                1,
                node::Features::NONE,
                alias2,
                1,
                &ua2,
                timestamp + 2,
                [],
            )
            .unwrap();
        assert!(updated);

        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.features, node::Features::NONE);
        assert_eq!(node.alias.as_ref(), "~alice~");
        assert_eq!(node.timestamp, timestamp + 2);
        assert_eq!(node.pow, 1);
        assert_eq!(node.addrs, vec![ka]);
        assert_eq!(node.agent, ua2);
    }

    #[test]
    fn test_insert_and_remove() {
        let alice = arbitrary::gen::<NodeId>(1);
        let bob = arbitrary::gen::<NodeId>(1);
        let mut cache = Database::memory().unwrap();
        let timestamp = LocalTime::now().into();
        let ua = UserAgent::default();
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
                banned: false,
            };
            cache
                .insert(
                    &alice,
                    1,
                    features,
                    alice_alias.clone(),
                    0,
                    &ua,
                    timestamp,
                    [ka.clone()],
                )
                .unwrap();
            cache
                .insert(
                    &bob,
                    1,
                    features,
                    bob_alias.clone(),
                    0,
                    &ua,
                    timestamp,
                    [ka],
                )
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
        let mut cache = Database::memory().unwrap();
        let mut expected = Vec::new();
        let timestamp = LocalTime::now().into();
        let ua = UserAgent::default();
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
                banned: false,
            };
            expected.push(AddressEntry {
                node: id,
                version: 3,
                penalty: Penalty::default(),
                address: ka.clone(),
            });
            cache
                .insert(&id, 3, features, alias.clone(), 0, &ua, timestamp, [ka])
                .unwrap();
        }

        let mut actual = cache.entries().unwrap().collect::<Vec<_>>();

        actual.sort_by_key(|ae| ae.node);
        expected.sort_by_key(|ae| ae.node);

        assert_eq!(cache.len().unwrap(), actual.len());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_disconnected() {
        let alice = arbitrary::gen::<NodeId>(1);
        let addr = arbitrary::gen::<Address>(1);
        let mut cache = Database::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = Timestamp::from(LocalTime::now());
        let ua = UserAgent::default();

        cache
            .insert(
                &alice,
                1,
                features,
                Alias::new("alice"),
                16,
                &ua,
                timestamp,
                [],
            )
            .unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.penalty, Penalty::default());

        cache.disconnected(&alice, &addr, Severity::Low).unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.penalty, Penalty::default());

        cache.disconnected(&alice, &addr, Severity::Medium).unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.penalty, Penalty(1));

        cache.disconnected(&alice, &addr, Severity::High).unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.penalty, Penalty(9));

        cache.connected(&alice, &addr, timestamp + 1).unwrap();
        let node = cache.get(&alice).unwrap().unwrap();
        assert_eq!(node.penalty, Penalty(4));
    }

    #[test]
    fn test_disconnected_ban() {
        let alice = arbitrary::gen::<NodeId>(1);
        let ua = UserAgent::default();
        let ip1: net::Ipv4Addr = [8, 8, 8, 8].into();
        let ip2: net::Ipv4Addr = [9, 9, 9, 9].into();
        let ka1 = arbitrary::gen::<KnownAddress>(1);
        let ka1 = KnownAddress {
            addr: Address::from(NetAddr::new(ip1.into(), 8776)),
            ..ka1
        };
        let ka2 = arbitrary::gen::<KnownAddress>(1);
        let ka2 = KnownAddress {
            addr: Address::from(NetAddr::new(ip2.into(), 8776)),
            ..ka2
        };
        let mut db = Database::memory().unwrap();
        let features = node::Features::SEED;
        let timestamp = Timestamp::from(LocalTime::now());

        db.insert(
            &alice,
            1,
            features,
            Alias::new("alice"),
            16,
            &ua,
            timestamp,
            [ka1.clone(), ka2.clone()],
        )
        .unwrap();
        db.record_ip(&alice, ip1.into(), timestamp).unwrap();
        db.record_ip(&alice, ip2.into(), timestamp).unwrap();

        let node = db.get(&alice).unwrap().unwrap();
        assert_eq!(node.penalty, Penalty::default());

        for _ in 0..7 {
            db.disconnected(&alice, &ka1.addr, Severity::High).unwrap();
            let node = db.get(&alice).unwrap().unwrap();

            assert!(!node.penalty.is_ban_threshold_reached());
            assert!(!node.banned);
        }

        db.disconnected(&alice, &ka1.addr, Severity::High).unwrap();
        let node = db.get(&alice).unwrap().unwrap();

        assert!(node.penalty.is_ban_threshold_reached());
        assert!(node.banned);

        for addr in node.addrs {
            if addr.addr == ka1.addr {
                assert!(addr.banned);
            } else {
                assert!(!addr.banned);
            }
        }
        assert!(db.is_addr_banned(&ka1.addr).unwrap());
        assert!(db.is_addr_banned(&ka2.addr).unwrap()); // Banned because node is banned.
        assert!(db.is_ip_banned(ip1.into()).unwrap());
        assert!(db.is_ip_banned(ip2.into()).unwrap());
    }
}
