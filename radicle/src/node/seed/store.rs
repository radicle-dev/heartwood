#![allow(clippy::type_complexity)]
use std::str::FromStr;

use localtime::LocalTime;
use sqlite as sql;
use thiserror::Error;

use crate::git::Oid;
use crate::node::address;
use crate::node::address::Store as _;
use crate::node::NodeId;
use crate::node::{seed::SyncedSeed, Database, SyncedAt};
use crate::prelude::{RepoId, Timestamp};

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// An address store error.
    #[error("address store error: {0}")]
    Addresses(#[from] address::Error),
}

/// Seed store.
///
/// Used to store seed sync statuses.
pub trait Store: address::Store {
    /// Mark a repo as synced on the given node.
    fn synced(
        &mut self,
        rid: &RepoId,
        nid: &NodeId,
        at: Oid,
        timestamp: Timestamp,
    ) -> Result<bool, Error>;
    /// Get the repos seeded by the given node.
    fn seeded_by(
        &self,
        nid: &NodeId,
    ) -> Result<Box<dyn Iterator<Item = Result<(RepoId, SyncedAt), Error>> + '_>, Error>;
    /// Get nodes that have synced the given repo.
    fn seeds_for(
        &self,
        rid: &RepoId,
    ) -> Result<Box<dyn Iterator<Item = Result<SyncedSeed, Error>> + '_>, Error>;
}

impl Store for Database {
    fn synced(
        &mut self,
        rid: &RepoId,
        nid: &NodeId,
        at: Oid,
        timestamp: Timestamp,
    ) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `repo-sync-status` (repo, node, head, timestamp)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO UPDATE
             SET head = ?3, timestamp = ?4
             WHERE timestamp < ?4 AND head <> ?3",
        )?;
        stmt.bind((1, rid))?;
        stmt.bind((2, nid))?;
        stmt.bind((3, at.to_string().as_str()))?;
        stmt.bind((4, &timestamp))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn seeds_for(
        &self,
        rid: &RepoId,
    ) -> Result<Box<dyn Iterator<Item = Result<SyncedSeed, Error>> + '_>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT node, head, timestamp
             FROM `repo-sync-status`
             WHERE repo = ?",
        )?;
        stmt.bind((1, rid))?;

        Ok(Box::new(stmt.into_iter().map(|row| {
            let row = row?;
            let nid = row.try_read::<NodeId, _>("node")?;
            let oid = row.try_read::<&str, _>("head")?;
            let oid = Oid::from_str(oid).map_err(|e| {
                Error::Internal(sql::Error {
                    code: None,
                    message: Some(format!("sql: invalid oid '{oid}': {e}")),
                })
            })?;
            let timestamp = row.try_read::<i64, _>("timestamp")?;
            let timestamp = LocalTime::from_millis(timestamp as u128);
            let addresses = self.addresses_of(&nid)?;

            Ok(SyncedSeed {
                nid,
                addresses,
                synced_at: SyncedAt { oid, timestamp },
            })
        })))
    }

    fn seeded_by(
        &self,
        nid: &NodeId,
    ) -> Result<Box<dyn Iterator<Item = Result<(RepoId, SyncedAt), Error>> + '_>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT repo, head, timestamp
             FROM `repo-sync-status`
             WHERE node = ?",
        )?;
        stmt.bind((1, nid))?;

        Ok(Box::new(stmt.into_iter().map(|row| {
            let row = row?;
            let rid = row.try_read::<RepoId, _>("repo")?;
            let oid = row.try_read::<&str, _>("head")?;
            let oid = Oid::from_str(oid).map_err(|e| {
                Error::Internal(sql::Error {
                    code: None,
                    message: Some(format!("sql: invalid oid '{oid}': {e}")),
                })
            })?;
            let timestamp = row.try_read::<i64, _>("timestamp")?;
            let timestamp = LocalTime::from_millis(timestamp as u128);

            Ok((rid, SyncedAt { oid, timestamp }))
        })))
    }
}
