#![allow(clippy::type_complexity)]
use std::num::TryFromIntError;
use std::str::FromStr;

use localtime::LocalTime;
use sqlite as sql;
use thiserror::Error;

use crate::git::{Oid, Qualified};
use crate::node::Database;
use crate::node::NodeId;
use crate::prelude::RepoId;
use crate::storage;
use crate::storage::{ReadRepository, ReadStorage, RemoteRepository, RepositoryError};

#[derive(Error, Debug)]
pub enum Error {
    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),
    /// Timestamp error.
    #[error("invalid timestamp: {0}")]
    Timestamp(#[from] TryFromIntError),
    /// Repository error.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
    /// Storage error.
    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),
    /// Storage refs error.
    #[error("storage refs error: {0}")]
    Refs(#[from] storage::refs::Error),
    /// No rows returned in query result.
    #[error("no rows returned")]
    NoRows,
}

/// Refs store.
///
/// Used to cache git references.
pub trait Store {
    /// Set a reference under a remote namespace to the given [`Oid`].
    fn set(
        &mut self,
        repo: &RepoId,
        namespace: &NodeId,
        refname: &Qualified,
        oid: Oid,
        timestamp: LocalTime,
    ) -> Result<bool, Error>;
    /// Get a reference's [`Oid`] and timestamp.
    fn get(
        &self,
        repo: &RepoId,
        namespace: &NodeId,
        refname: &Qualified,
    ) -> Result<Option<(Oid, LocalTime)>, Error>;
    /// Delete a reference.
    fn delete(
        &mut self,
        repo: &RepoId,
        namespace: &NodeId,
        refname: &Qualified,
    ) -> Result<bool, Error>;
    /// Populate the database from storage.
    fn populate<S: ReadStorage>(&mut self, storage: &S) -> Result<(), Error>;
    /// Return the number of references.
    fn count(&self) -> Result<usize, Error>;
    /// Check if there are any references.
    fn is_empty(&self) -> Result<bool, Error> {
        self.count().map(|l| l == 0)
    }
}

impl Store for Database {
    fn set(
        &mut self,
        repo: &RepoId,
        namespace: &NodeId,
        refname: &Qualified,
        oid: Oid,
        timestamp: LocalTime,
    ) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `refs` (repo, namespace, ref, oid, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO UPDATE
             SET oid = ?4, timestamp = ?5
             WHERE timestamp < ?5 AND oid <> ?4",
        )?;
        stmt.bind((1, repo))?;
        stmt.bind((2, namespace))?;
        stmt.bind((3, refname.to_string().as_str()))?;
        stmt.bind((4, oid.to_string().as_str()))?;
        stmt.bind((5, i64::try_from(timestamp.as_millis())?))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn get(
        &self,
        repo: &RepoId,
        namespace: &NodeId,
        refname: &Qualified,
    ) -> Result<Option<(Oid, LocalTime)>, Error> {
        let mut stmt = self.db.prepare(
            "SELECT oid, timestamp FROM refs WHERE repo = ?1 AND namespace = ?2 AND ref = ?3",
        )?;

        stmt.bind((1, repo))?;
        stmt.bind((2, namespace))?;
        stmt.bind((3, refname.to_string().as_str()))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let oid = row.try_read::<&str, _>("oid")?;
            let oid = Oid::from_str(oid).map_err(|e| {
                Error::Internal(sql::Error {
                    code: None,
                    message: Some(format!("sql: invalid oid '{oid}': {e}")),
                })
            })?;
            let timestamp = row.try_read::<i64, _>("timestamp")?;
            let timestamp = LocalTime::from_millis(timestamp as u128);

            Ok(Some((oid, timestamp)))
        } else {
            Ok(None)
        }
    }

    fn delete(
        &mut self,
        repo: &RepoId,
        namespace: &NodeId,
        refname: &Qualified,
    ) -> Result<bool, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM refs WHERE repo = ?1 AND namespace = ?2 AND ref = ?3")?;

        stmt.bind((1, repo))?;
        stmt.bind((2, namespace))?;
        stmt.bind((3, refname.to_string().as_str()))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    fn count(&self) -> Result<usize, Error> {
        let row = self
            .db
            .prepare("SELECT COUNT(*) FROM refs")?
            .into_iter()
            .next()
            .ok_or(Error::NoRows)??;
        let count = row.read::<i64, _>(0) as usize;

        Ok(count)
    }

    fn populate<S: ReadStorage>(&mut self, storage: &S) -> Result<(), Error> {
        let now = LocalTime::now();

        for info in storage.repositories()? {
            let repo = storage.repository(info.rid)?;
            for refs_at in repo.remote_refs_at()? {
                self.set(&repo.id(), &refs_at.remote, refs_at.path(), refs_at.at, now)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::git::qualified;
    use crate::test::arbitrary;
    use localtime::{LocalDuration, LocalTime};

    #[test]
    fn test_count() {
        let mut db = Database::memory().unwrap();
        let oid = arbitrary::oid();

        let repo = arbitrary::gen::<RepoId>(1);
        let namespace = arbitrary::gen::<NodeId>(1);
        let refname1 = qualified!("refs/heads/master");
        let refname2 = qualified!("refs/heads/main");
        let timestamp = LocalTime::now();

        assert!(db.is_empty().unwrap());
        assert_eq!(db.count().unwrap(), 0);

        assert!(db
            .set(&repo, &namespace, &refname1, oid, timestamp)
            .unwrap());
        assert!(!db.is_empty().unwrap());
        assert_eq!(db.count().unwrap(), 1);

        assert!(db
            .set(&repo, &namespace, &refname2, oid, timestamp)
            .unwrap());
        assert_eq!(db.count().unwrap(), 2);
    }

    #[test]
    fn test_set_and_delete() {
        let mut db = Database::memory().unwrap();
        let oid = arbitrary::oid();

        let repo = arbitrary::gen::<RepoId>(1);
        let namespace = arbitrary::gen::<NodeId>(1);
        let refname = qualified!("refs/heads/master");
        let timestamp = LocalTime::now();

        assert!(db.set(&repo, &namespace, &refname, oid, timestamp).unwrap());
        assert!(db.get(&repo, &namespace, &refname).unwrap().is_some());
        assert!(db.delete(&repo, &namespace, &refname).unwrap());
        assert!(db.get(&repo, &namespace, &refname).unwrap().is_none());
        assert!(!db.delete(&repo, &namespace, &refname).unwrap());
    }

    #[test]
    fn test_set_and_get() {
        let mut db = Database::memory().unwrap();
        let oid1 = arbitrary::oid();
        let oid2 = arbitrary::oid();

        assert_ne!(oid1, oid2);

        let repo = arbitrary::gen::<RepoId>(1);
        let namespace = arbitrary::gen::<NodeId>(1);
        let refname = qualified!("refs/heads/master");
        let mut timestamp = LocalTime::now();

        assert_eq!(db.get(&repo, &namespace, &refname).unwrap(), None);
        assert!(db
            .set(&repo, &namespace, &refname, oid1, timestamp)
            .unwrap());
        assert_eq!(
            db.get(&repo, &namespace, &refname).unwrap(),
            Some((oid1, timestamp))
        );
        assert!(!db
            .set(&repo, &namespace, &refname, oid1, timestamp)
            .unwrap());
        timestamp.elapse(LocalDuration::from_millis(1));

        assert!(db
            .set(&repo, &namespace, &refname, oid2, timestamp)
            .unwrap());
        assert_eq!(
            db.get(&repo, &namespace, &refname).unwrap(),
            Some((oid2, timestamp))
        );
    }
}
