#![allow(clippy::type_complexity)]
use std::path::Path;
use std::str::FromStr;
use std::{fmt, io};

use sqlite as sql;
use thiserror::Error;

use crate::prelude::Id;
use crate::service::NodeId;

use super::{Alias, Policy, Scope};

#[derive(Error, Debug)]
pub enum Error {
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
}

impl sqlite::BindableWithIndex for Scope {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        let s = match self {
            Self::Trusted => "trusted",
            Self::DelegatesOnly => "delegates-only",
            Self::All => "all",
        };
        s.bind(stmt, i)
    }
}

impl TryFrom<&sql::Value> for Scope {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        let message = Some("invalid remote scope".to_owned());

        match value {
            sql::Value::String(scope) => Scope::from_str(scope).map_err(|_| sql::Error {
                code: None,
                message,
            }),
            _ => Err(sql::Error {
                code: None,
                message,
            }),
        }
    }
}

impl sqlite::BindableWithIndex for Policy {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        match self {
            Self::Track => "track",
            Self::Block => "block",
        }
        .bind(stmt, i)
    }
}

impl TryFrom<&sql::Value> for Policy {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        let message = Some("sql: invalid policy".to_owned());

        match value {
            sql::Value::String(s) if s == "track" => Ok(Policy::Track),
            sql::Value::String(s) if s == "block" => Ok(Policy::Block),
            _ => Err(sql::Error {
                code: None,
                message,
            }),
        }
    }
}

/// Tracking configuration.
pub struct Config {
    db: sql::Connection,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Config(..)")
    }
}

impl Config {
    const SCHEMA: &str = include_str!("schema.sql");

    /// Open a policy store at the given path. Creates a new store if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let db = sql::Connection::open(path)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open(":memory:")?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Track a node.
    pub fn track_node(&mut self, id: &NodeId, alias: Option<&str>) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `node-policies` (id, alias)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET alias = ?2 WHERE alias != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, alias.unwrap_or_default()))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Track a repository.
    pub fn track_repo(&mut self, id: &Id, scope: Scope) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `repo-policies` (id, scope)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET scope = ?2 WHERE scope != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, scope))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Set a node's tracking policy.
    pub fn set_node_policy(&mut self, id: &NodeId, policy: Policy) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `node-policies` (id, policy)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET policy = ?2 WHERE policy != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, policy))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Set a repository's tracking policy.
    pub fn set_repo_policy(&mut self, id: &Id, policy: Policy) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `repo-policies` (id, policy)
             VALUES (?1, ?2)
             ON CONFLICT DO UPDATE
             SET policy = ?2 WHERE policy != ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, policy))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Untrack a node.
    pub fn untrack_node(&mut self, id: &NodeId) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM `node-policies` WHERE id = ?")?;

        stmt.bind((1, id))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Untrack a repository.
    pub fn untrack_repo(&mut self, id: &Id) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM `repo-policies` WHERE id = ?")?;

        stmt.bind((1, id))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Check if a node is tracked.
    pub fn is_node_tracked(&self, id: &NodeId) -> Result<bool, Error> {
        Ok(matches!(self.node_entry(id)?, Some((_, Policy::Track))))
    }

    /// Check if a repository is tracked.
    pub fn is_repo_tracked(&self, id: &Id) -> Result<bool, Error> {
        Ok(matches!(self.repo_entry(id)?, Some((_, Policy::Track))))
    }

    /// Get a node's tracking information.
    pub fn node_entry(&self, id: &NodeId) -> Result<Option<(Option<Alias>, Policy)>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT alias, policy FROM `node-policies` WHERE id = ?")?;

        stmt.bind((1, id))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let alias = row.read::<&str, _>("alias");

            return Ok(Some((
                if alias.is_empty() {
                    None
                } else {
                    Some(alias.to_owned())
                },
                row.read::<Policy, _>("policy"),
            )));
        }
        Ok(None)
    }

    /// Get a repository's tracking information.
    pub fn repo_entry(&self, id: &Id) -> Result<Option<(Scope, Policy)>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT scope, policy FROM `repo-policies` WHERE id = ?")?;

        stmt.bind((1, id))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            return Ok(Some((
                row.read::<Scope, _>("scope"),
                row.read::<Policy, _>("policy"),
            )));
        }
        Ok(None)
    }

    /// Get node tracking entries.
    pub fn node_entries(&self) -> Result<Box<dyn Iterator<Item = (NodeId, Alias, Policy)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT id, alias, policy FROM `node-policies`")?
            .into_iter();
        let mut entries = Vec::new();

        while let Some(Ok(row)) = stmt.next() {
            let id = row.read("id");
            let alias = row.read::<&str, _>("alias");
            let policy = row.read::<Policy, _>("policy");

            entries.push((id, alias.to_owned(), policy));
        }
        Ok(Box::new(entries.into_iter()))
    }

    /// Get repository tracking entries.
    pub fn repo_entries(&self) -> Result<Box<dyn Iterator<Item = (Id, Scope, Policy)>>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT id, scope, policy FROM `repo-policies`")?
            .into_iter();
        let mut entries = Vec::new();

        while let Some(Ok(row)) = stmt.next() {
            let id = row.read("id");
            let scope = row.read("scope");
            let policy = row.read::<Policy, _>("policy");

            entries.push((id, scope, policy));
        }
        Ok(Box::new(entries.into_iter()))
    }
}

#[cfg(test)]
mod test {
    use radicle::assert_matches;

    use super::*;
    use crate::test::arbitrary;

    #[test]
    fn test_track_and_untrack_node() {
        let id = arbitrary::gen::<NodeId>(1);
        let mut db = Config::open(":memory:").unwrap();

        assert!(db.track_node(&id, Some("eve")).unwrap());
        assert!(db.is_node_tracked(&id).unwrap());
        assert!(!db.track_node(&id, Some("eve")).unwrap());
        assert!(db.untrack_node(&id).unwrap());
        assert!(!db.is_node_tracked(&id).unwrap());
    }

    #[test]
    fn test_track_and_untrack_repo() {
        let id = arbitrary::gen::<Id>(1);
        let mut db = Config::open(":memory:").unwrap();

        assert!(db.track_repo(&id, Scope::All).unwrap());
        assert!(db.is_repo_tracked(&id).unwrap());
        assert!(!db.track_repo(&id, Scope::All).unwrap());
        assert!(db.untrack_repo(&id).unwrap());
        assert!(!db.is_repo_tracked(&id).unwrap());
    }

    #[test]
    fn test_node_entries() {
        let ids = arbitrary::vec::<NodeId>(3);
        let mut db = Config::open(":memory:").unwrap();

        for id in &ids {
            assert!(db.track_node(id, None).unwrap());
        }
        let mut entries = db.node_entries().unwrap();
        assert_matches!(entries.next(), Some((id, _, _)) if id == ids[0]);
        assert_matches!(entries.next(), Some((id, _, _)) if id == ids[1]);
        assert_matches!(entries.next(), Some((id, _, _)) if id == ids[2]);
    }

    #[test]
    fn test_repo_entries() {
        let ids = arbitrary::vec::<Id>(3);
        let mut db = Config::open(":memory:").unwrap();

        for id in &ids {
            assert!(db.track_repo(id, Scope::All).unwrap());
        }
        let mut entries = db.repo_entries().unwrap();
        assert_matches!(entries.next(), Some((id, _, _)) if id == ids[0]);
        assert_matches!(entries.next(), Some((id, _, _)) if id == ids[1]);
        assert_matches!(entries.next(), Some((id, _, _)) if id == ids[2]);
    }

    #[test]
    fn test_update_alias() {
        let id = arbitrary::gen::<NodeId>(1);
        let mut db = Config::open(":memory:").unwrap();

        assert!(db.track_node(&id, Some("eve")).unwrap());
        assert_eq!(
            db.node_entry(&id).unwrap().unwrap().0,
            Some(String::from("eve"))
        );
        assert!(db.track_node(&id, None).unwrap());
        assert_eq!(db.node_entry(&id).unwrap().unwrap().0, None);
        assert!(!db.track_node(&id, None).unwrap());
        assert!(db.track_node(&id, Some("alice")).unwrap());
        assert_eq!(
            db.node_entry(&id).unwrap().unwrap().0,
            Some(String::from("alice"))
        );
    }

    #[test]
    fn test_update_scope() {
        let id = arbitrary::gen::<Id>(1);
        let mut db = Config::open(":memory:").unwrap();

        assert!(db.track_repo(&id, Scope::All).unwrap());
        assert_eq!(db.repo_entry(&id).unwrap().unwrap().0, Scope::All);
        assert!(db.track_repo(&id, Scope::DelegatesOnly).unwrap());
        assert_eq!(db.repo_entry(&id).unwrap().unwrap().0, Scope::DelegatesOnly);
    }

    #[test]
    fn test_repo_policy() {
        let id = arbitrary::gen::<Id>(1);
        let mut db = Config::open(":memory:").unwrap();

        assert!(db.track_repo(&id, Scope::All).unwrap());
        assert_eq!(db.repo_entry(&id).unwrap().unwrap().1, Policy::Track);
        assert!(db.set_repo_policy(&id, Policy::Block).unwrap());
        assert_eq!(db.repo_entry(&id).unwrap().unwrap().1, Policy::Block);
    }

    #[test]
    fn test_node_policy() {
        let id = arbitrary::gen::<NodeId>(1);
        let mut db = Config::open(":memory:").unwrap();

        assert!(db.track_node(&id, None).unwrap());
        assert_eq!(db.node_entry(&id).unwrap().unwrap().1, Policy::Track);
        assert!(db.set_node_policy(&id, Policy::Block).unwrap());
        assert_eq!(db.node_entry(&id).unwrap().unwrap().1, Policy::Block);
    }
}
