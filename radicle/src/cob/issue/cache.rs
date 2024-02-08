use std::str::FromStr;

use sqlite as sql;
use thiserror::Error;

use crate::cob;
use crate::cob::cache;
use crate::cob::cache::{Remove, StoreReader, StoreWriter, Update};
use crate::cob::store;
use crate::cob::{Embed, Label, ObjectId, TypeName};
use crate::crypto::Signer;
use crate::prelude::{Did, RepoId};
use crate::sql::transaction;
use crate::storage::{ReadRepository, RepositoryError, SignRepository, WriteRepository};

use super::{Issue, IssueCounts, IssueId, IssueMut, State};

pub trait Issues {
    type Error: std::error::Error + Send + Sync + 'static;
    type Iter<'a>: Iterator<Item = Result<(IssueId, Issue), Self::Error>> + 'a
    where
        Self: 'a;

    fn get(&self, id: &IssueId) -> Result<Option<Issue>, Self::Error>;
    fn list(&self) -> Result<Self::Iter<'_>, Self::Error>;
    fn counts(&self) -> Result<IssueCounts, Self::Error>;

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.counts()?.total() == 0)
    }
}

pub trait IssuesMut: Issues + Update<Issue> + Remove<Issue> {}

impl<T> IssuesMut for T where T: Issues + Update<Issue> + Remove<Issue> {}

pub struct Cache<'a, R, C> {
    store: super::Issues<'a, R>,
    cache: C,
}

impl<'a, R, C> Cache<'a, R, C>
where
    R: ReadRepository,
{
    pub fn rid(&self) -> RepoId {
        self.store.raw.as_ref().id()
    }
}

impl<'a, R> Cache<'a, R, StoreReader>
where
    R: ReadRepository + cob::Store,
{
    pub fn reader(repository: &'a R, cache: StoreReader) -> Result<Self, RepositoryError> {
        let store = super::Issues::open(repository)?;
        Ok(Self { store, cache })
    }
}

impl<'a, R> Cache<'a, R, StoreWriter>
where
    R: ReadRepository + cob::Store,
{
    pub fn open(repository: &'a R, cache: StoreWriter) -> Result<Self, RepositoryError> {
        let store = super::Issues::open(repository)?;
        Ok(Self { store, cache })
    }

    pub fn create<'g, G>(
        &'g mut self,
        title: impl ToString,
        description: impl ToString,
        labels: &[Label],
        assignees: &[Did],
        embeds: impl IntoIterator<Item = Embed>,
        signer: &G,
    ) -> Result<IssueMut<'a, 'g, R, StoreWriter>, super::Error>
    where
        R: WriteRepository,
        G: Signer,
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

    pub fn remove<G>(&mut self, id: &IssueId, signer: &G) -> Result<(), store::Error>
    where
        G: Signer,
        R: SignRepository,
    {
        self.store.remove(id, &mut self.cache, signer)
    }
}

impl<'a, R, C> cache::Update<Issue> for Cache<'a, R, C>
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

impl<'a, R, C> cache::Remove<Issue> for Cache<'a, R, C>
where
    C: cache::Remove<Issue>,
{
    type Out = <C as cache::Remove<Issue>>::Out;
    type RemoveError = <C as cache::Remove<Issue>>::RemoveError;

    fn remove(&mut self, rid: &RepoId, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        self.cache.remove(rid, id)
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
                "INSERT INTO issues (id, repo_id, issue)
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

    fn remove(&mut self, rid: &RepoId, id: &ObjectId) -> Result<Self::Out, Self::RemoveError> {
        transaction::<_, sql::Error>(&self.db, move |db| {
            let mut stmt = db.prepare(
                "DELETE FROM issues
                  WHERE  repo_id =  ?1
                  AND id = ?2",
            )?;

            stmt.bind((1, rid))?;
            stmt.bind((2, sql::Value::String(id.to_string())))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("object `{1}` of type `{0}` was not found")]
    NotFound(TypeName, ObjectId),
    #[error(transparent)]
    Object(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sql(#[from] sql::Error),
}

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

impl<'a, R> Issues for Cache<'a, R, StoreWriter>
where
    R: ReadRepository,
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

impl<'a, R> Issues for Cache<'a, R, StoreReader>
where
    R: ReadRepository,
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
             WHERE id = ?1 and repo_id = ?2",
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
            "SELECT id, issue FROM issues
              WHERE repo_id = ?1
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
                 issue->'$.state' as state,
                 COUNT(*) as count
             FROM issues
             WHERE repo_id = ?1
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
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::str::FromStr;

    use radicle_cob::ObjectId;

    use crate::cob;
    use crate::cob::cache::{Store, Update, Write};
    use crate::cob::thread::Thread;
    use crate::issue::{CloseReason, Issue, IssueCounts, IssueId, State};
    use crate::test::arbitrary;
    use crate::test::storage::{MockRepository, ReadRepository as _};

    use super::{Cache, Issues};

    fn memory(repo: &MockRepository) -> Cache<'_, MockRepository, Store<Write>> {
        let store = cob::issue::Issues::open(repo).unwrap();
        let cache = Store::<Write>::memory().unwrap();
        Cache { store, cache }
    }

    #[test]
    fn test_is_empty() {
        let repo = arbitrary::gen::<MockRepository>(1);
        let mut cache = memory(&repo);
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
        let mut cache = memory(&repo);
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
        let mut cache = memory(&repo);
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
        let mut cache = memory(&repo);
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
        let mut cache = memory(&repo);
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
            super::Remove::remove(&mut cache, &repo.id(), id).unwrap();
            assert_eq!(None, cache.get(id).unwrap());
        }
    }
}
