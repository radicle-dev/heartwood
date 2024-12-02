use std::num::TryFromIntError;
use std::{fmt, io};

use radicle::crypto::Signature;
use sqlite as sql;
use thiserror::Error;

use crate::node::{Database, NodeId};
use crate::prelude::{Filter, Timestamp};
use crate::service::message::{
    Announcement, AnnouncementMessage, InventoryAnnouncement, NodeAnnouncement, RefsAnnouncement,
};
use crate::wire;
use crate::wire::Decode;

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// Unit overflow.
    #[error("unit overflow:: {0}")]
    UnitOverflow(#[from] TryFromIntError),
}

/// Unique announcement identifier.
pub type AnnouncementId = u64;

/// A database that has access to historical gossip messages.
/// Keeps track of the latest received gossip messages for each node.
/// Grows linearly with the number of nodes on the network.
pub trait Store {
    /// Prune announcements older than the cutoff time.
    fn prune(&mut self, cutoff: Timestamp) -> Result<usize, Error>;

    /// Get the timestamp of the last announcement in the store.
    fn last(&self) -> Result<Option<Timestamp>, Error>;

    /// Process an announcement for the given node.
    /// Returns `true` if the timestamp was updated or the announcement wasn't there before.
    fn announced(
        &mut self,
        nid: &NodeId,
        ann: &Announcement,
    ) -> Result<Option<AnnouncementId>, Error>;

    /// Set whether a message should be relayed or not.
    fn set_relay(&mut self, id: AnnouncementId, relay: RelayStatus) -> Result<(), Error>;

    /// Return messages that should be relayed.
    fn relays(&mut self, now: Timestamp) -> Result<Vec<(AnnouncementId, Announcement)>, Error>;

    /// Get all the latest gossip messages of all nodes, filtered by inventory filter and
    /// announcement timestamps.
    ///
    /// # Panics
    ///
    /// Panics if `from` > `to`.
    ///
    fn filtered<'a>(
        &'a self,
        filter: &'a Filter,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Box<dyn Iterator<Item = Result<Announcement, Error>> + 'a>, Error>;
}

impl Store for Database {
    fn prune(&mut self, cutoff: Timestamp) -> Result<usize, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM `announcements` WHERE timestamp < ?1")?;

        stmt.bind((1, &cutoff))?;
        stmt.next()?;

        Ok(self.db.change_count())
    }

    fn last(&self) -> Result<Option<Timestamp>, Error> {
        let stmt = self
            .db
            .prepare("SELECT MAX(timestamp) AS latest FROM `announcements`")?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            return match row.try_read::<Option<i64>, _>(0)? {
                Some(i) => Ok(Some(Timestamp::try_from(i)?)),
                None => Ok(None),
            };
        }
        Ok(None)
    }

    fn announced(
        &mut self,
        nid: &NodeId,
        ann: &Announcement,
    ) -> Result<Option<AnnouncementId>, Error> {
        assert_ne!(
            ann.timestamp(),
            Timestamp::MIN,
            "Timestamp of {ann:?} must not be zero"
        );
        let mut stmt = self.db.prepare(
            "INSERT INTO `announcements` (node, repo, type, message, signature, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT DO UPDATE
             SET message = ?4, signature = ?5, timestamp = ?6
             WHERE timestamp < ?6
             RETURNING rowid",
        )?;
        stmt.bind((1, nid))?;

        match &ann.message {
            AnnouncementMessage::Node(msg) => {
                stmt.bind((2, sql::Value::String(String::new())))?;
                stmt.bind((3, &GossipType::Node))?;
                stmt.bind((4, msg))?;
            }
            AnnouncementMessage::Refs(msg) => {
                stmt.bind((2, &msg.rid))?;
                stmt.bind((3, &GossipType::Refs))?;
                stmt.bind((4, msg))?;
            }
            AnnouncementMessage::Inventory(msg) => {
                stmt.bind((2, sql::Value::String(String::new())))?;
                stmt.bind((3, &GossipType::Inventory))?;
                stmt.bind((4, msg))?;
            }
        }
        stmt.bind((5, &ann.signature))?;
        stmt.bind((6, &ann.message.timestamp()))?;

        if let Some(row) = stmt.into_iter().next() {
            let row = row?;
            let id = row.read::<i64, _>("rowid");

            Ok(Some(id as AnnouncementId))
        } else {
            Ok(None)
        }
    }

    fn set_relay(&mut self, id: AnnouncementId, relay: RelayStatus) -> Result<(), Error> {
        let mut stmt = self.db.prepare(
            "UPDATE announcements
             SET relay = ?1
             WHERE rowid = ?2",
        )?;
        stmt.bind((1, relay))?;
        stmt.bind((2, id as i64))?;
        stmt.next()?;

        Ok(())
    }

    fn relays(&mut self, now: Timestamp) -> Result<Vec<(AnnouncementId, Announcement)>, Error> {
        let mut stmt = self.db.prepare(
            "UPDATE announcements
             SET relay = ?1
             WHERE relay IS ?2
             RETURNING rowid, node, type, message, signature, timestamp",
        )?;
        stmt.bind((1, RelayStatus::RelayedAt(now)))?;
        stmt.bind((2, RelayStatus::Relay))?;

        let mut rows = stmt
            .into_iter()
            .map(|row| {
                let row = row?;
                parse::announcement(row)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Nb. Manually sort by insertion order, because we can't use `ORDER BY` with `RETURNING`
        // as of SQLite 3.45.
        rows.sort_by_key(|(id, _)| *id);

        Ok(rows)
    }

    fn filtered<'a>(
        &'a self,
        filter: &'a Filter,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Box<dyn Iterator<Item = Result<Announcement, Error>> + 'a>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT rowid, node, type, message, signature, timestamp
             FROM announcements
             WHERE timestamp >= ?1 and timestamp < ?2
             ORDER BY timestamp, node, type",
        )?;
        assert!(*from <= *to);

        stmt.bind((1, &from))?;
        stmt.bind((2, &to))?;

        Ok(Box::new(
            stmt.into_iter()
                .map(|row| {
                    let row = row?;
                    let (_, ann) = parse::announcement(row)?;

                    Ok(ann)
                })
                .filter(|ann| match ann {
                    Ok(a) => a.matches(filter),
                    Err(_) => true,
                }),
        ))
    }
}

impl TryFrom<&sql::Value> for NodeAnnouncement {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::Binary(bytes) => {
                let mut reader = io::Cursor::new(bytes);
                NodeAnnouncement::decode(&mut reader).map_err(wire::Error::into)
            }
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for node announcement".to_owned()),
            }),
        }
    }
}

impl sql::BindableWithIndex for &NodeAnnouncement {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        wire::serialize(self).bind(stmt, i)
    }
}

impl TryFrom<&sql::Value> for RefsAnnouncement {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::Binary(bytes) => {
                let mut reader = io::Cursor::new(bytes);
                RefsAnnouncement::decode(&mut reader).map_err(wire::Error::into)
            }
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for refs announcement".to_owned()),
            }),
        }
    }
}

impl sql::BindableWithIndex for &RefsAnnouncement {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        wire::serialize(self).bind(stmt, i)
    }
}

impl TryFrom<&sql::Value> for InventoryAnnouncement {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::Binary(bytes) => {
                let mut reader = io::Cursor::new(bytes);
                InventoryAnnouncement::decode(&mut reader).map_err(wire::Error::into)
            }
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for inventory announcement".to_owned()),
            }),
        }
    }
}

impl sql::BindableWithIndex for &InventoryAnnouncement {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        wire::serialize(self).bind(stmt, i)
    }
}

impl From<wire::Error> for sql::Error {
    fn from(other: wire::Error) -> Self {
        sql::Error {
            code: None,
            message: Some(other.to_string()),
        }
    }
}

/// Message relay status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayStatus {
    Relay,
    DontRelay,
    RelayedAt(Timestamp),
}

impl sql::BindableWithIndex for RelayStatus {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        match self {
            Self::Relay => sql::Value::Null.bind(stmt, i),
            Self::DontRelay => sql::Value::Integer(-1).bind(stmt, i),
            Self::RelayedAt(t) => t.bind(stmt, i),
        }
    }
}

/// Type of gossip message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GossipType {
    Refs,
    Node,
    Inventory,
}

impl fmt::Display for GossipType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refs => write!(f, "refs"),
            Self::Node => write!(f, "node"),
            Self::Inventory => write!(f, "inventory"),
        }
    }
}

impl sql::BindableWithIndex for &GossipType {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        self.to_string().as_str().bind(stmt, i)
    }
}

impl TryFrom<&sql::Value> for GossipType {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::String(s) => match s.as_str() {
                "refs" => Ok(Self::Refs),
                "node" => Ok(Self::Node),
                "inventory" => Ok(Self::Inventory),
                other => Err(sql::Error {
                    code: None,
                    message: Some(format!("unknown gossip type '{other}'")),
                }),
            },
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for gossip type".to_owned()),
            }),
        }
    }
}

mod parse {
    use super::*;

    pub fn announcement(row: sql::Row) -> Result<(AnnouncementId, Announcement), Error> {
        let id = row.read::<i64, _>("rowid") as AnnouncementId;
        let node = row.read::<NodeId, _>("node");
        let gt = row.read::<GossipType, _>("type");
        let message = match gt {
            GossipType::Refs => {
                let ann = row.try_read::<RefsAnnouncement, _>("message")?;
                AnnouncementMessage::Refs(ann)
            }
            GossipType::Inventory => {
                let ann = row.try_read::<InventoryAnnouncement, _>("message")?;
                AnnouncementMessage::Inventory(ann)
            }
            GossipType::Node => {
                let ann = row.try_read::<NodeAnnouncement, _>("message")?;
                AnnouncementMessage::Node(ann)
            }
        };
        let signature = row.read::<Signature, _>("signature");
        let timestamp = row.read::<Timestamp, _>("timestamp");

        debug_assert_eq!(timestamp, message.timestamp());

        Ok((
            id,
            Announcement {
                node,
                message,
                signature,
            },
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;
    use crate::prelude::{BoundedVec, RepoId};
    use crate::test::arbitrary;
    use localtime::LocalTime;
    use radicle::assert_matches;
    use radicle::node::device::Device;

    #[test]
    fn test_announced() {
        let mut db = Database::memory().unwrap();
        let nid = arbitrary::gen::<NodeId>(1);
        let rid = arbitrary::gen::<RepoId>(1);
        let timestamp = LocalTime::now().into();
        let signer = Device::mock();
        let refs = AnnouncementMessage::Refs(RefsAnnouncement {
            rid,
            refs: BoundedVec::new(),
            timestamp,
        })
        .signed(&signer);
        let inv = AnnouncementMessage::Inventory(InventoryAnnouncement {
            inventory: BoundedVec::new(),
            timestamp,
        })
        .signed(&signer);

        // Only the first announcement of each type is recognized as new.
        let id1 = db.announced(&nid, &refs).unwrap().unwrap();
        assert!(db.announced(&nid, &refs).unwrap().is_none());

        let id2 = db.announced(&nid, &inv).unwrap().unwrap();
        assert!(db.announced(&nid, &inv).unwrap().is_none());

        // Nothing was set to be relayed.
        assert_eq!(db.relays(LocalTime::now().into()).unwrap().len(), 0);

        // Set the messages to be relayed.
        db.set_relay(id1, RelayStatus::Relay).unwrap();
        db.set_relay(id2, RelayStatus::Relay).unwrap();

        // Now they are returned.
        assert_matches!(
            db.relays(LocalTime::now().into()).unwrap().as_slice(),
            &[(id1_, _), (id2_, _)]
            if id1_ == id1 && id2_ == id2
        );
        // But only once.
        assert_matches!(db.relays(LocalTime::now().into()).unwrap().as_slice(), &[]);
    }
}
