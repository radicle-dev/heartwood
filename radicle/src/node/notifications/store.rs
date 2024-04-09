#![allow(clippy::type_complexity)]
use std::marker::PhantomData;
use std::num::TryFromIntError;
use std::path::Path;
use std::sync::Arc;
use std::{fmt, io, str::FromStr, time};

use localtime::LocalTime;
use sqlite as sql;
use thiserror::Error;

use crate::git;
use crate::git::{Oid, RefError, RefString};
use crate::prelude::RepoId;
use crate::sql::transaction;
use crate::storage::RefUpdate;

use super::{
    Notification, NotificationId, NotificationKind, NotificationKindError, NotificationStatus,
};

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
    /// Timestamp error.
    #[error("invalid timestamp: {0}")]
    Timestamp(#[from] TryFromIntError),
    /// Invalid Git ref name.
    #[error("invalid ref name: {0}")]
    RefName(#[from] RefError),
    /// Invalid Git ref format.
    #[error("invalid ref format: {0}")]
    RefFormat(#[from] git_ext::ref_format::Error),
    /// Invalid notification kind.
    #[error("invalid notification kind: {0}")]
    NotificationKind(#[from] NotificationKindError),
    /// Not found.
    #[error("notification {0} not found")]
    NotificationNotFound(NotificationId),
    /// Internal unit overflow.
    #[error("the unit overflowed")]
    UnitOverflow,
}

/// Read-only type witness.
#[derive(Clone)]
pub struct Read;
/// Read-write type witness.
#[derive(Clone)]
pub struct Write;

/// Notifications store.
#[derive(Clone)]
pub struct Store<T> {
    db: Arc<sql::ConnectionThreadSafe>,
    marker: PhantomData<T>,
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
        let mut db = sql::Connection::open_thread_safe_with_flags(
            path,
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.set_busy_timeout(DB_READ_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open_thread_safe_with_flags(
            ":memory:",
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }
}

impl Store<Write> {
    const SCHEMA: &'static str = include_str!("schema.sql");

    /// Open a policy store at the given path. Creates a new store if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut db = sql::Connection::open_thread_safe(path)?;
        db.set_busy_timeout(DB_WRITE_TIMEOUT.as_millis() as usize)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, Error> {
        let db = sql::Connection::open_thread_safe(":memory:")?;
        db.execute(Self::SCHEMA)?;

        Ok(Self {
            db: Arc::new(db),
            marker: PhantomData,
        })
    }

    /// Get a read-only version of this store.
    pub fn read_only(self) -> Store<Read> {
        Store {
            db: self.db,
            marker: PhantomData,
        }
    }

    /// Set notification read status for the given notifications.
    pub fn set_status(
        &mut self,
        status: NotificationStatus,
        ids: &[NotificationId],
    ) -> Result<bool, Error> {
        transaction(&self.db, |_| {
            let mut stmt = self.db.prepare(
                "UPDATE `repository-notifications`
                 SET status = ?1
                 WHERE rowid = ?2",
            )?;
            for id in ids {
                stmt.bind((1, &status))?;
                stmt.bind((2, *id as i64))?;
                stmt.next()?;
                stmt.reset()?;
            }
            Ok(self.db.change_count() > 0)
        })
    }

    /// Insert a notification. Resets the status to *unread* if it already exists.
    pub fn insert(
        &mut self,
        repo: &RepoId,
        update: &RefUpdate,
        timestamp: LocalTime,
    ) -> Result<bool, Error> {
        let mut stmt = self.db.prepare(
            "INSERT INTO `repository-notifications` (repo, ref, old, new, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO UPDATE
             SET old = ?3, new = ?4, timestamp = ?5, status = null",
        )?;
        let old = update.old().map(|o| o.to_string());
        let new = update.new().map(|o| o.to_string());

        stmt.bind((1, repo))?;
        stmt.bind((2, update.name().as_str()))?;
        stmt.bind((3, old.as_deref()))?;
        stmt.bind((4, new.as_deref()))?;
        stmt.bind((5, i64::try_from(timestamp.as_millis())?))?;
        stmt.next()?;

        Ok(self.db.change_count() > 0)
    }

    /// Delete the given notifications.
    pub fn clear(&mut self, ids: &[NotificationId]) -> Result<usize, Error> {
        transaction(&self.db, |_| {
            let mut stmt = self
                .db
                .prepare("DELETE FROM `repository-notifications` WHERE rowid = ?")?;

            // N.b. we need to keep the count manually since the change count
            // will always be `1` because of each reset.
            let mut count = 0;
            for id in ids {
                stmt.bind((1, *id as i64))?;
                stmt.next()?;
                stmt.reset()?;
                count += self.db.change_count();
            }
            Ok(count)
        })
    }

    /// Delete all notifications of a repo.
    pub fn clear_by_repo(&mut self, repo: &RepoId) -> Result<usize, Error> {
        let mut stmt = self
            .db
            .prepare("DELETE FROM `repository-notifications` WHERE repo = ?")?;

        stmt.bind((1, repo))?;
        stmt.next()?;

        Ok(self.db.change_count())
    }

    /// Delete all notifications from all repos.
    pub fn clear_all(&mut self) -> Result<usize, Error> {
        self.db
            .prepare("DELETE FROM `repository-notifications`")?
            .next()?;

        Ok(self.db.change_count())
    }
}

/// `Read` methods for `Store`. This implies that a
/// `Store<Write>` can access these functions as well.
impl<T> Store<T> {
    /// Get a specific notification.
    pub fn get(&self, id: NotificationId) -> Result<Notification, Error> {
        let mut stmt = self.db.prepare(
            "SELECT rowid, repo, ref, old, new, status, timestamp
             FROM `repository-notifications`
             WHERE rowid = ?",
        )?;
        stmt.bind((1, id as i64))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            return parse::notification(row);
        }
        Err(Error::NotificationNotFound(id))
    }

    /// Get all notifications.
    pub fn all(&self) -> Result<impl Iterator<Item = Result<Notification, Error>> + '_, Error> {
        let stmt = self.db.prepare(
            "SELECT rowid, repo, ref, old, new, status, timestamp
             FROM `repository-notifications`
             ORDER BY timestamp DESC",
        )?;

        Ok(stmt.into_iter().map(move |row| {
            let row = row?;
            parse::notification(row)
        }))
    }

    // Get notifications that were created between the given times: `since <= t < until`.
    pub fn by_timestamp(
        &self,
        since: LocalTime,
        until: LocalTime,
    ) -> Result<impl Iterator<Item = Result<Notification, Error>> + '_, Error> {
        let mut stmt = self.db.prepare(
            "SELECT rowid, repo, ref, old, new, status, timestamp
             FROM `repository-notifications`
             WHERE timestamp >= ?1 AND timestamp < ?2
             ORDER BY timestamp",
        )?;
        let since = i64::try_from(since.as_millis())?;
        let until = i64::try_from(until.as_millis())?;

        stmt.bind((1, since))?;
        stmt.bind((2, until))?;

        Ok(stmt.into_iter().map(move |row| {
            let row = row?;
            parse::notification(row)
        }))
    }

    /// Get notifications by repo.
    pub fn by_repo(
        &self,
        repo: &RepoId,
        order_by: &str,
    ) -> Result<impl Iterator<Item = Result<Notification, Error>> + '_, Error> {
        let mut stmt = self.db.prepare(format!(
            "SELECT rowid, repo, ref, old, new, status, timestamp
             FROM `repository-notifications`
             WHERE repo = ?
             ORDER BY {order_by} DESC",
        ))?;
        stmt.bind((1, repo))?;

        Ok(stmt.into_iter().map(move |row| {
            let row = row?;
            parse::notification(row)
        }))
    }

    /// Get the total notification count.
    pub fn count(&self) -> Result<usize, Error> {
        let stmt = self
            .db
            .prepare("SELECT COUNT(*) FROM `repository-notifications`")?;

        let count: i64 = stmt
            .into_iter()
            .next()
            .expect("COUNT will always return a single row")?
            .read(0);
        let count: usize = count.try_into().map_err(|_| Error::UnitOverflow)?;

        Ok(count)
    }

    /// Get the notification for the given repo.
    pub fn count_by_repo(&self, repo: &RepoId) -> Result<usize, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT COUNT(*) FROM `repository-notifications` WHERE repo = ?")?;

        stmt.bind((1, repo))?;

        let count: i64 = stmt
            .into_iter()
            .next()
            .expect("COUNT will always return a single row")?
            .read(0);
        let count: usize = count.try_into().map_err(|_| Error::UnitOverflow)?;

        Ok(count)
    }
}

mod parse {
    use super::*;

    pub fn notification(row: sql::Row) -> Result<Notification, Error> {
        let id = row.try_read::<i64, _>("rowid")? as NotificationId;
        let repo = row.try_read::<RepoId, _>("repo")?;
        let refstr = row.try_read::<&str, _>("ref")?;
        let status = row.try_read::<NotificationStatus, _>("status")?;
        let old = row
            .try_read::<Option<&str>, _>("old")?
            .map(|oid| {
                Oid::from_str(oid).map_err(|e| {
                    Error::Internal(sql::Error {
                        code: None,
                        message: Some(format!("sql: invalid oid in `old` column: {oid:?}: {e}")),
                    })
                })
            })
            .unwrap_or(Ok(git::raw::Oid::zero().into()))?;
        let new = row
            .try_read::<Option<&str>, _>("new")?
            .map(|oid| {
                Oid::from_str(oid).map_err(|e| {
                    Error::Internal(sql::Error {
                        code: None,
                        message: Some(format!("sql: invalid oid in `new` column: {oid:?}: {e}")),
                    })
                })
            })
            .unwrap_or(Ok(git::raw::Oid::zero().into()))?;
        let update = RefUpdate::from(RefString::try_from(refstr)?, old, new);
        let (namespace, qualified) = git::parse_ref(refstr)?;
        let timestamp = row.try_read::<i64, _>("timestamp")?;
        let timestamp = LocalTime::from_millis(timestamp as u128);
        let qualified = qualified.to_owned();
        let kind = NotificationKind::try_from(qualified.clone())?;

        Ok(Notification {
            id,
            repo,
            update,
            remote: namespace,
            qualified,
            status,
            kind,
            timestamp,
        })
    }
}

#[cfg(test)]
mod test {
    use radicle_git_ext::ref_format::{qualified, refname};

    use super::*;
    use crate::{cob, node::NodeId, test::arbitrary};

    #[test]
    fn test_clear() {
        let mut db = Store::open(":memory:").unwrap();
        let repo = arbitrary::gen::<RepoId>(1);
        let old = arbitrary::oid();
        let time = LocalTime::from_millis(32188142);
        let master = arbitrary::oid();

        for i in 0..3 {
            let update = RefUpdate::Updated {
                name: format!("refs/heads/feature/{i}").try_into().unwrap(),
                old,
                new: master,
            };
            assert!(db.insert(&repo, &update, time).unwrap());
        }
        assert_eq!(db.count().unwrap(), 3);
        assert_eq!(db.count_by_repo(&repo).unwrap(), 3);
        db.clear_by_repo(&repo).unwrap();
        assert_eq!(db.count().unwrap(), 0);
        assert_eq!(db.count_by_repo(&repo).unwrap(), 0);
    }

    #[test]
    fn test_branch_notifications() {
        let repo = arbitrary::gen::<RepoId>(1);
        let old = arbitrary::oid();
        let master = arbitrary::oid();
        let other = arbitrary::oid();
        let time1 = LocalTime::from_millis(32188142);
        let time2 = LocalTime::from_millis(32189874);
        let time3 = LocalTime::from_millis(32189879);
        let mut db = Store::open(":memory:").unwrap();

        let update1 = RefUpdate::Updated {
            name: refname!("refs/heads/master"),
            old,
            new: master,
        };
        let update2 = RefUpdate::Created {
            name: refname!("refs/heads/other"),
            oid: other,
        };
        let update3 = RefUpdate::Deleted {
            name: refname!("refs/heads/dev"),
            oid: other,
        };
        assert!(db.insert(&repo, &update1, time1).unwrap());
        assert!(db.insert(&repo, &update2, time2).unwrap());
        assert!(db.insert(&repo, &update3, time3).unwrap());

        let mut notifs = db.by_repo(&repo, "timestamp").unwrap();

        assert_eq!(
            notifs.next().unwrap().unwrap(),
            Notification {
                id: 3,
                repo,
                remote: None,
                qualified: qualified!("refs/heads/dev"),
                update: update3,
                kind: NotificationKind::Branch {
                    name: refname!("dev")
                },
                status: NotificationStatus::Unread,
                timestamp: time3,
            }
        );
        assert_eq!(
            notifs.next().unwrap().unwrap(),
            Notification {
                id: 2,
                repo,
                remote: None,
                qualified: qualified!("refs/heads/other"),
                update: update2,
                kind: NotificationKind::Branch {
                    name: refname!("other")
                },
                status: NotificationStatus::Unread,
                timestamp: time2,
            }
        );
        assert_eq!(
            notifs.next().unwrap().unwrap(),
            Notification {
                id: 1,
                repo,
                remote: None,
                qualified: qualified!("refs/heads/master"),
                update: update1,
                kind: NotificationKind::Branch {
                    name: refname!("master")
                },
                status: NotificationStatus::Unread,
                timestamp: time1,
            }
        );
        assert!(notifs.next().is_none());
    }

    #[test]
    fn test_notification_status() {
        let repo = arbitrary::gen::<RepoId>(1);
        let oid = arbitrary::oid();
        let time = LocalTime::from_millis(32188142);
        let mut db = Store::open(":memory:").unwrap();

        let update1 = RefUpdate::Created {
            name: refname!("refs/heads/feature/1"),
            oid,
        };
        let update2 = RefUpdate::Created {
            name: refname!("refs/heads/feature/2"),
            oid,
        };
        let update3 = RefUpdate::Created {
            name: refname!("refs/heads/feature/3"),
            oid,
        };
        assert!(db.insert(&repo, &update1, time).unwrap());
        assert!(db.insert(&repo, &update2, time).unwrap());
        assert!(db.insert(&repo, &update3, time).unwrap());
        assert!(db
            .set_status(NotificationStatus::ReadAt(time), &[1, 2, 3])
            .unwrap());

        let mut notifs = db.by_repo(&repo, "timestamp").unwrap();

        assert_eq!(
            notifs.next().unwrap().unwrap().status,
            NotificationStatus::ReadAt(time),
        );
        assert_eq!(
            notifs.next().unwrap().unwrap().status,
            NotificationStatus::ReadAt(time),
        );
        assert_eq!(
            notifs.next().unwrap().unwrap().status,
            NotificationStatus::ReadAt(time),
        );
    }

    #[test]
    fn test_duplicate_notifications() {
        let repo = arbitrary::gen::<RepoId>(1);
        let old = arbitrary::oid();
        let master1 = arbitrary::oid();
        let master2 = arbitrary::oid();
        let time1 = LocalTime::from_millis(32188142);
        let time2 = LocalTime::from_millis(32189874);
        let mut db = Store::open(":memory:").unwrap();

        let update1 = RefUpdate::Updated {
            name: refname!("refs/heads/master"),
            old,
            new: master1,
        };
        let update2 = RefUpdate::Updated {
            name: refname!("refs/heads/master"),
            old: master1,
            new: master2,
        };
        assert!(db.insert(&repo, &update1, time1).unwrap());
        assert!(db
            .set_status(NotificationStatus::ReadAt(time1), &[1])
            .unwrap());
        assert!(db.insert(&repo, &update2, time2).unwrap());

        let mut notifs = db.by_repo(&repo, "timestamp").unwrap();

        assert_eq!(
            notifs.next().unwrap().unwrap(),
            Notification {
                id: 1,
                repo,
                remote: None,
                qualified: qualified!("refs/heads/master"),
                update: update2,
                kind: NotificationKind::Branch {
                    name: refname!("master")
                },
                // Status is reset to "unread".
                status: NotificationStatus::Unread,
                timestamp: time2,
            }
        );
        assert!(notifs.next().is_none());
    }

    #[test]
    fn test_cob_notifications() {
        let repo = arbitrary::gen::<RepoId>(1);
        let old = arbitrary::oid();
        let new = arbitrary::oid();
        let timestamp = LocalTime::from_millis(32189874);
        let nid: NodeId = "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
            .parse()
            .unwrap();
        let mut db = Store::open(":memory:").unwrap();
        let qualified =
            qualified!("refs/cobs/xyz.radicle.issue/d87dcfe8c2b3200e78b128d9b959cfdf7063fefe");
        let namespaced = qualified.with_namespace((&nid).into());
        let update = RefUpdate::Updated {
            name: namespaced.to_ref_string(),
            old,
            new,
        };

        assert!(db.insert(&repo, &update, timestamp).unwrap());

        let mut notifs = db.by_repo(&repo, "timestamp").unwrap();

        assert_eq!(
            notifs.next().unwrap().unwrap(),
            Notification {
                id: 1,
                repo,
                remote: Some(
                    "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
                        .parse()
                        .unwrap()
                ),
                qualified,
                update,
                kind: NotificationKind::Cob {
                    typed_id: cob::TypedId {
                        type_name: cob::issue::TYPENAME.clone(),
                        id: "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe".parse().unwrap(),
                    },
                },
                status: NotificationStatus::Unread,
                timestamp,
            }
        );
        assert!(notifs.next().is_none());
    }
}
