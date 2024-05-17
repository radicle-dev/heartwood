use std::ops::ControlFlow;
use std::str::FromStr;

use sqlite as sql;
use thiserror::Error;

use crate::cob;
use crate::cob::cache;
use crate::cob::cache::{Remove, StoreReader, StoreWriter, Update};
use crate::cob::store;
use crate::cob::{Embed, Label, ObjectId, TypeName};
use crate::crypto::Signer;
use crate::identity;
use crate::prelude::{Did, RepoId};
use crate::sql::transaction;
use crate::storage::{HasRepoId, ReadRepository, RepositoryError, SignRepository, WriteRepository};

use super::{Issue, IssueCounts, IssueId, IssueMut, State};

/// A set of read-only methods for a [`Issue`] store.
pub trait Issues {
    type Error: std::error::Error + Send + Sync + 'static;

    /// An iterator for returning a set of issues from the store.
    type Iter<'a>: Iterator<Item = Result<(IssueId, Issue), Self::Error>> + 'a
    where
        Self: 'a;

    /// Get the `Issue`, identified by `id`, returning `None` if it
    /// was not found.
    fn get(&self, id: &IssueId) -> Result<Option<Issue>, Self::Error>;

    /// List all issues that are in the store.
    fn list(&self) -> Result<Self::Iter<'_>, Self::Error>;

    /// Get the [`IssueCounts`] of all the issues in the store.
    fn counts(&self) -> Result<IssueCounts, Self::Error>;

    /// Returns `true` if there are no issues in the store.
    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.counts()?.total() == 0)
    }
}

pub trait IssuesExt: Issues {
    /// Iterator of all `IssueId`s returned by [`IssuesExt::ids`].
    type Ids<'a>: Iterator<Item = Result<IssueId, Self::Error>> + 'a
    where
        Self: 'a;

    /// Iterator of all `Did`s returned by [`IssuesExt::assignees`].
    type Dids<'a>: Iterator<Item = Result<Did, Self::Error>> + 'a
    where
        Self: 'a;

    /// Query for the list of all `IssueId`s that start with `prefix`.
    fn ids(&self, prefix: &str) -> Result<Self::Ids<'_>, Self::Error>;

    /// Query for the list of all assignees' `Did`s that start with `prefix`.
    fn assignees(&self, prefix: &str) -> Result<Self::Dids<'_>, Self::Error>;
}

/// [`Issues`] store that can also [`Update`] and [`Remove`]
/// [`Issue`] in/from the store.
pub trait IssuesMut: Issues + Update<Issue> + Remove<Issue> {}

impl<T> IssuesMut for T where T: Issues + Update<Issue> + Remove<Issue> {}

/// An `Issue` store that relies on the `cache` for reads and as a
/// write-through cache.
///
/// The `store` is used for the main storage when performing a
/// write-through. It is also used for identifying which `RepoId` is
/// being used for the `cache`.
pub struct Cache<R, C> {
    store: R,
    cache: C,
}

impl<R, C> Cache<R, C> {
    pub fn new(store: R, cache: C) -> Self {
        Self { store, cache }
    }

    pub fn rid(&self) -> RepoId
    where
        R: HasRepoId,
    {
        self.store.rid()
    }
}

impl<'a, R, C> Cache<super::Issues<'a, R>, C> {
    /// Create a new [`Issue`] using the [`super::Issues`] as the
    /// main storage, and writing the update to the `cache`.
    pub fn create<'g, G>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        labels: &[Label],
        assignees: &[Did],
        embeds: impl IntoIterator<Item = Embed>,
        signer: &G,
    ) -> Result<IssueMut<'a, 'g, R, C>, super::Error>
    where
        R: ReadRepository + WriteRepository + cob::Store,
        G: Signer,
        C: Update<Issue>,
    {
        self.store.create(
            title,
            description,
            labels,
            assignees,
            embeds,
            &mut self.cache,
            signer,
        )
    }

    /// Remove the given `id` from the [`super::Issues`] storage, and
    /// removing the entry from the `cache`.
    pub fn remove<G>(&mut self, id: &IssueId, signer: &G) -> Result<(), super::Error>
    where
        G: Signer,
        R: ReadRepository + SignRepository + cob::Store,
        C: Remove<Issue>,
    {
        self.store.remove(id, signer)?;
        self.cache
            .remove(id)
            .map_err(|e| super::Error::CacheRemove {
                id: *id,
                err: e.into(),
            })?;
        Ok(())
    }

    /// Read the given `id` from the [`super::Issues`] store and
    /// writing it to the `cache`.
    pub fn write(&mut self, id: &IssueId) -> Result<(), super::Error>
    where
        R: ReadRepository + cob::Store,
        C: Update<Issue>,
    {
        let issue = self
            .store
            .get(id)?
            .ok_or_else(|| store::Error::NotFound((*super::TYPENAME).clone(), *id))?;
        self.update(&self.rid(), id, &issue)
            .map_err(|e| super::Error::CacheUpdate {
                id: *id,
                err: e.into(),
            })?;
        Ok(())
    }

    /// Read all the issues from the [`super::Issues`] store and
    /// writing them to `cache`.
    ///
    /// The `callback` is used for reporting success, failures, and
    /// progress to the caller. The caller may also decide to continue
    /// or break from the process.
    pub fn write_all(
        &mut self,
        on_issue: impl Fn(
            &Result<(IssueId, Issue), store::Error>,
            &cache::WriteAllProgress,
        ) -> ControlFlow<()>,
    ) -> Result<(), super::Error>
    where
        R: ReadRepository + cob::Store,
        C: Update<Issue>,
    {
        let issues = self.store.all()?;
        let mut progress = cache::WriteAllProgress::new(issues.len());
        for issue in self.store.all()? {
            progress.inc();
            match on_issue(&issue, &progress) {
                ControlFlow::Continue(()) => match issue {
                    Ok((id, issue)) => {
                        self.update(&self.rid(), &id, &issue)
                            .map_err(|e| super::Error::CacheUpdate { id, err: e.into() })?;
                    }
                    Err(_) => continue,
                },
                ControlFlow::Break(()) => break,
            }
        }
        Ok(())
    }
}

impl<'a, R> Cache<super::Issues<'a, R>, cache::NoCache>
where
    R: ReadRepository + cob::Store,
{
    /// Get a `Cache` that does no write-through modifications and
    /// uses the [`super::Issues`] store for all reads and writes.
    pub fn no_cache(repository: &'a R) -> Result<Self, RepositoryError> {
        let store = super::Issues::open(repository)?;
        Ok(Self {
            store,
            cache: cache::NoCache,
        })
    }

    /// Get the [`IssueMut`], identified by `id`.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<IssueMut<'a, 'g, R, cache::NoCache>, super::Error> {
        let issue = self
            .store
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(super::TYPENAME.clone(), *id))?;

        Ok(IssueMut {
            id: *id,
            issue,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }
}

impl<R> Cache<R, StoreReader> {
    pub fn reader(store: R, cache: StoreReader) -> Self {
        Self { store, cache }
    }
}

impl<R> Cache<R, StoreWriter> {
    pub fn open(store: R, cache: StoreWriter) -> Self {
        Self { store, cache }
    }
}

impl<'a, R> Cache<super::Issues<'a, R>, StoreWriter>
where
    R: ReadRepository + cob::Store,
{
    /// Get the [`IssueMut`], identified by `id`, using the
    /// `StoreWriter` for retrieving the `Issue`.
    pub fn get_mut<'g>(
        &'g mut self,
        id: &ObjectId,
    ) -> Result<IssueMut<'a, 'g, R, StoreWriter>, Error> {
        let issue = Issues::get(self, id)?
            .ok_or_else(move || Error::NotFound(super::TYPENAME.clone(), *id))?;

        Ok(IssueMut {
            id: *id,
            issue,
            store: &mut self.store,
            cache: &mut self.cache,
        })
    }
}

impl<R, C> cache::Update<Issue> for Cache<R, C>
where
    C: cache::Update<Issue>,
{
    type Out = <C as cache::Update<Issue>>::Out;
    type UpdateError = <C as cache::Update<Issue>>::UpdateError;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &Issue,
    ) -> Result<Self::Out, Self::UpdateError> {
        self.cache.update(rid, id, object)
    }
}

impl<R, C> cache::Remove<Issue> for Cache<R, C>
where
    C: cache::Remove<Issue>,
{
    type Out = <C as cache::Remove<Issue>>::Out;
    type RemoveError = <C as cache::Remove<Issue>>::RemoveError;

    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        self.cache.remove(id)
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

impl Update<Issue> for StoreWriter {
    type Out = bool;
    type UpdateError = UpdateError;

    fn update(
        &mut self,
        rid: &RepoId,
        id: &ObjectId,
        object: &Issue,
    ) -> Result<Self::Out, Self::UpdateError> {
        transaction::<_, UpdateError>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "INSERT INTO issues (id, repo, issue)
                  VALUES (?1, ?2, ?3)
                  ON CONFLICT DO UPDATE
                  SET issue =  (?3)",
            )?;

            stmt.bind((1, sql::Value::String(id.to_string())))?;
            stmt.bind((2, rid))?;
            stmt.bind((3, sql::Value::String(serde_json::to_string(&object)?)))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

impl Remove<Issue> for StoreWriter {
    type Out = bool;
    type RemoveError = sql::Error;

    fn remove(&mut self, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        transaction::<_, sql::Error>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "DELETE FROM issues
                  WHERE id = ?1",
            )?;

            stmt.bind((1, sql::Value::String(id.to_string())))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

pub struct NoCacheIter<'a> {
    inner: Box<dyn Iterator<Item = Result<(IssueId, Issue), super::Error>> + 'a>,
}

impl<'a> Iterator for NoCacheIter<'a> {
    type Item = Result<(IssueId, Issue), super::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, R> Issues for Cache<super::Issues<'a, R>, cache::NoCache>
where
    R: ReadRepository + cob::Store,
{
    type Error = super::Error;
    type Iter<'b> = NoCacheIter<'b> where Self: 'b;

    fn get(&self, id: &IssueId) -> Result<Option<Issue>, Self::Error> {
        self.store.get(id).map_err(super::Error::from)
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        self.store
            .all()
            .map(|inner| NoCacheIter {
                inner: Box::new(inner.into_iter().map(|res| res.map_err(super::Error::from))),
            })
            .map_err(super::Error::from)
    }

    fn counts(&self) -> Result<IssueCounts, Self::Error> {
        self.store.counts().map_err(super::Error::from)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
    #[error(transparent)]
    Object(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    Did(#[from] identity::did::DidError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

/// Iterator that returns a set of issues based on an SQL query.
///
/// The query is expected to return rows with columns identified by
/// the `id` and `issue` names.
pub struct IssuesIter<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> IssuesIter<'a> {
    fn parse_row(row: sql::Row) -> Result<(IssueId, Issue), Error> {
        let id = IssueId::from_str(row.read::<&str, _>("id"))?;
        let issue = serde_json::from_str::<Issue>(row.read::<&str, _>("issue"))?;
        Ok((id, issue))
    }
}

impl<'a> Iterator for IssuesIter<'a> {
    type Item = Result<(IssueId, Issue), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(IssuesIter::parse_row))
    }
}

/// Iterator that returns the IDs for issues based on an SQL query.
///
/// The query is expected to return rows with a column identified by
/// the `id` name.
pub struct IssueIds<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> IssueIds<'a> {
    fn parse_row(row: sql::Row) -> Result<IssueId, Error> {
        let id = IssueId::from_str(row.read::<&str, _>("id"))?;
        Ok(id)
    }
}

impl<'a> Iterator for IssueIds<'a> {
    type Item = Result<IssueId, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(IssueIds::parse_row))
    }
}

/// Iterator that returns the DIDs of issues' assignees based on an SQL query.
///
/// The query is expected to return rows with a column identified by
/// the `did` name.
pub struct Dids<'a> {
    inner: sql::CursorWithOwnership<'a>,
}

impl<'a> Dids<'a> {
    fn parse_row(row: sql::Row) -> Result<Did, Error> {
        let did = Did::from_str(row.read::<&str, _>("did"))?;
        Ok(did)
    }
}

impl<'a> Iterator for Dids<'a> {
    type Item = Result<Did, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let row = self.inner.next()?;
        Some(row.map_err(Error::from).and_then(Dids::parse_row))
    }
}

impl<R> Issues for Cache<R, StoreWriter>
where
    R: HasRepoId,
{
    type Error = Error;
    type Iter<'b> = IssuesIter<'b> where Self: 'b;

    fn get(&self, id: &IssueId) -> Result<Option<Issue>, Self::Error> {
        query::get(&self.cache.db, &self.rid(), id)
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        query::list(&self.cache.db, &self.rid())
    }

    fn counts(&self) -> Result<IssueCounts, Self::Error> {
        query::counts(&self.cache.db, &self.rid())
    }
}

impl<R> IssuesExt for Cache<R, StoreWriter>
where
    R: HasRepoId,
{
    type Ids<'a> = IssueIds<'a> where Self: 'a;
    type Dids<'a> = Dids<'a> where Self: 'a;

    fn ids(&self, prefix: &str) -> Result<Self::Ids<'_>, Self::Error> {
        query::ids(&self.cache.db, prefix, &self.rid())
    }

    fn assignees(&self, prefix: &str) -> Result<Self::Dids<'_>, Self::Error> {
        query::dids(&self.cache.db, prefix, &self.rid())
    }
}

impl<R> Issues for Cache<R, StoreReader>
where
    R: HasRepoId,
{
    type Error = Error;
    type Iter<'b> = IssuesIter<'b> where Self: 'b;

    fn get(&self, id: &IssueId) -> Result<Option<Issue>, Self::Error> {
        query::get(&self.cache.db, &self.rid(), id)
    }

    fn list(&self) -> Result<Self::Iter<'_>, Self::Error> {
        query::list(&self.cache.db, &self.rid())
    }

    fn counts(&self) -> Result<IssueCounts, Self::Error> {
        query::counts(&self.cache.db, &self.rid())
    }
}

impl<R> IssuesExt for Cache<R, StoreReader>
where
    R: HasRepoId,
{
    type Ids<'a> = IssueIds<'a> where Self: 'a;
    type Dids<'a> = Dids<'a> where Self: 'a;

    fn ids(&self, prefix: &str) -> Result<Self::Ids<'_>, Self::Error> {
        query::ids(&self.cache.db, prefix, &self.rid())
    }

    fn assignees(&self, prefix: &str) -> Result<Self::Dids<'_>, Self::Error> {
        query::dids(&self.cache.db, prefix, &self.rid())
    }
}

/// Helper SQL queries for [ `Issues`] trait implementations.
mod query {
    use sqlite as sql;

    use super::*;

    pub(super) fn get(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
        id: &IssueId,
    ) -> Result<Option<Issue>, Error> {
        let id = sql::Value::String(id.to_string());
        let mut stmt = db.prepare(
            "SELECT issue
             FROM issues
             WHERE id = ?1 and repo = ?2",
        )?;

        stmt.bind((1, id))?;
        stmt.bind((2, rid))?;

        match stmt.into_iter().next().transpose()? {
            None => Ok(None),
            Some(row) => {
                let issue = row.read::<&str, _>("issue");
                let issue = serde_json::from_str(issue)?;
                Ok(Some(issue))
            }
        }
    }

    pub(super) fn list<'a>(
        db: &'a sql::ConnectionThreadSafe,
        rid: &RepoId,
    ) -> Result<IssuesIter<'a>, Error> {
        let mut stmt = db.prepare(
            "SELECT id, issue
             FROM issues
             WHERE repo = ?1
            ",
        )?;
        stmt.bind((1, rid))?;
        Ok(IssuesIter {
            inner: stmt.into_iter(),
        })
    }

    pub(super) fn counts(
        db: &sql::ConnectionThreadSafe,
        rid: &RepoId,
    ) -> Result<IssueCounts, Error> {
        let mut stmt = db.prepare(
            "SELECT
                 issue->'$.state' AS state,
                 COUNT(*) AS count
             FROM issues
             WHERE repo = ?1
             GROUP BY issue->'$.state.status'",
        )?;
        stmt.bind((1, rid))?;

        stmt.into_iter()
            .try_fold(IssueCounts::default(), |mut counts, row| {
                let row = row?;
                let count = row.read::<i64, _>("count") as usize;
                let status = serde_json::from_str::<State>(row.read::<&str, _>("state"))?;
                match status {
                    State::Closed { .. } => counts.closed += count,
                    State::Open => counts.open += count,
                }
                Ok(counts)
            })
    }

    pub(super) fn ids<'a>(
        db: &'a sql::ConnectionThreadSafe,
        prefix: &str,
        rid: &RepoId,
    ) -> Result<IssueIds<'a>, Error> {
        let mut stmt = db.prepare(
            "SELECT id
             FROM issues
             WHERE repo = ?1
             AND id LIKE ?2
            ",
        )?;
        stmt.bind((1, rid))?;
        stmt.bind((2, sql::Value::String(format!("{}%", prefix))))?;
        Ok(IssueIds {
            inner: stmt.into_iter(),
        })
    }

    pub(super) fn dids<'a>(
        db: &'a sql::ConnectionThreadSafe,
        prefix: &str,
        rid: &RepoId,
    ) -> Result<Dids<'a>, Error> {
        let mut stmt = db.prepare(
            "SELECT issues.assignees as did
             FROM issues, json_each(issues.assignees)
             WHERE repo = ?1
             AND json.value LIKE %?2
            ",
        )?;
        stmt.bind((1, rid))?;
        stmt.bind((2, sql::Value::String(format!("{}%", prefix))))?;
        Ok(Dids {
            inner: stmt.into_iter(),
        })
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::str::FromStr;

    use radicle_cob::ObjectId;

    use crate::cob::cache::{Store, Update, Write};
    use crate::cob::thread::Thread;
    use crate::issue::{CloseReason, Issue, IssueCounts, IssueId, State};
    use crate::prelude::Did;
    use crate::test::arbitrary;
    use crate::test::storage::MockRepository;

    use super::{Cache, Issues, IssuesExt};

    fn memory(store: MockRepository) -> Cache<MockRepository, Store<Write>> {
        let cache = Store::<Write>::memory().unwrap();
        Cache { store, cache }
    }

    #[test]
    fn test_is_empty() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        assert!(cache.is_empty().unwrap());

        let issue = Issue::new(Thread::default());
        let id = ObjectId::from_str("47799cbab2eca047b6520b9fce805da42b49ecab").unwrap();
        cache.update(&cache.rid(), &id, &issue).unwrap();

        let issue = Issue {
            state: State::Closed {
                reason: CloseReason::Solved,
            },
            ..Issue::new(Thread::default())
        };
        let id = ObjectId::from_str("ae981ded6ed2ed2cdba34c8603714782667f18a3").unwrap();
        cache.update(&cache.rid(), &id, &issue).unwrap();

        assert!(!cache.is_empty().unwrap())
    }

    #[test]
    fn test_counts() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        let n_open = arbitrary::gen::<u8>(0);
        let n_closed = arbitrary::gen::<u8>(1);
        let open_ids = (0..n_open)
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();
        let closed_ids = (0..n_closed)
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();

        for id in open_ids.iter() {
            let issue = Issue::new(Thread::default());
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
        }

        for id in closed_ids.iter() {
            let issue = Issue {
                state: State::Closed {
                    reason: CloseReason::Solved,
                },
                ..Issue::new(Thread::default())
            };
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
        }

        assert_eq!(
            cache.counts().unwrap(),
            IssueCounts {
                open: open_ids.len(),
                closed: closed_ids.len()
            }
        );
    }

    #[test]
    fn test_get() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();
        let missing = (0..arbitrary::gen::<u8>(2))
            .filter_map(|_| {
                let id = IssueId::from(arbitrary::oid());
                (!ids.contains(&id)).then_some(id)
            })
            .collect::<BTreeSet<IssueId>>();
        let mut issues = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let issue = Issue {
                title: id.to_string(),
                ..Issue::new(Thread::default())
            };
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
            issues.push((*id, issue));
        }

        for (id, issue) in issues.into_iter() {
            assert_eq!(Some(issue), cache.get(&id).unwrap());
        }

        for id in &missing {
            assert_eq!(cache.get(id).unwrap(), None);
        }
    }

    #[test]
    fn test_list() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();
        let mut issues = Vec::with_capacity(ids.len());

        for id in ids.iter() {
            let issue = Issue {
                title: id.to_string(),
                ..Issue::new(Thread::default())
            };
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
            issues.push((*id, issue));
        }

        let mut list = cache
            .list()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        list.sort_by_key(|(id, _)| *id);
        issues.sort_by_key(|(id, _)| *id);
        assert_eq!(issues, list);
    }

    #[test]
    fn test_remove() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();

        for id in ids.iter() {
            let issue = Issue {
                title: id.to_string(),
                ..Issue::new(Thread::default())
            };
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
            assert_eq!(Some(issue), cache.get(id).unwrap());
            super::Remove::remove(&mut cache, id).unwrap();
            assert_eq!(None, cache.get(id).unwrap());
        }
    }

    #[test]
    fn test_ids() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();

        for id in ids.iter() {
            let issue = Issue {
                title: id.to_string(),
                ..Issue::new(Thread::default())
            };
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
            let mut ids = cache.ids(&id.to_string()[..7]).unwrap();
            assert_eq!(ids.next().expect("no Issue Id was returned").unwrap(), *id);
        }
    }

    #[test]
    fn test_assignees() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(repo);
        let ids = (0..arbitrary::gen::<u8>(1))
            .map(|_| IssueId::from(arbitrary::oid()))
            .collect::<BTreeSet<IssueId>>();
        let dids = arbitrary::gen::<Vec<Did>>(1)
            .into_iter()
            .collect::<BTreeSet<_>>();

        for (id, did) in ids.iter().zip(dids.clone()) {
            let assignees = [did].into_iter().collect();
            let issue = Issue {
                title: id.to_string(),
                assignees,
                ..Issue::new(Thread::default())
            };
            cache
                .update(&cache.rid(), &IssueId::from(*id), &issue)
                .unwrap();
        }

        for did in dids {
            let mut dids = cache.assignees(&did.to_string()[..7]).unwrap();
            assert_eq!(dids.next().expect("no DID was returned").unwrap(), did);
        }
    }
}
