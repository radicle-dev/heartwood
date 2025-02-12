use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use radicle::cob::store::Cob;
use radicle::cob::{self, store, EntryId, Evaluate, ObjectId, Op, TypeName};
use radicle::crypto::Signer;
use radicle::node::NodeId;
use radicle::prelude::ReadRepository;
use radicle::storage::{RepositoryError, SignRepository, WriteRepository};
use radicle::{cob::store::CobAction, git::Oid};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

pub mod error;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radworks.job").expect("type name is valid"));

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    oid: Oid,
    runs: HashMap<NodeId, Runs>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runs(IndexMap<Uuid, Run>);

impl Runs {
    pub fn insert(&mut self, uuid: Uuid, run: Run) -> Option<Run> {
        self.0.insert(uuid, run)
    }

    pub fn contains_key(&self, uuid: &Uuid) -> bool {
        self.0.contains_key(uuid)
    }

    pub fn latest(&self) -> Option<(&Uuid, &Run)> {
        self.0.iter().next_back()
    }

    pub fn started(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.is_started().then_some((*uuid, run.clone())))
            .collect()
    }

    pub fn finished(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.is_finished().then_some((*uuid, run.clone())))
            .collect()
    }

    pub fn succeeded(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.succeeded().then_some((*uuid, run.clone())))
            .collect()
    }

    pub fn failed(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.failed().then_some((*uuid, run.clone())))
            .collect()
    }

    pub fn partition(&self) -> (Runs, Runs, Runs) {
        let mut started = IndexMap::new();
        let mut succeeded = IndexMap::new();
        let mut failed = IndexMap::new();

        for (uuid, run) in self.0.iter() {
            match run.status {
                Status::Started => started.insert(*uuid, run.clone()),
                Status::Finished(Reason::Succeeded) => succeeded.insert(*uuid, run.clone()),
                Status::Finished(Reason::Failed) => failed.insert(*uuid, run.clone()),
            };
        }
        (Runs(started), Runs(succeeded), Runs(failed))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Uuid, &Run)> {
        self.0.iter()
    }
}

impl FromIterator<(Uuid, Run)> for Runs {
    fn from_iter<T: IntoIterator<Item = (Uuid, Run)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a> IntoIterator for &'a Runs {
    type Item = (&'a Uuid, &'a Run);
    type IntoIter = indexmap::map::Iter<'a, Uuid, Run>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for Runs {
    type Item = (Uuid, Run);
    type IntoIter = indexmap::map::IntoIter<Uuid, Run>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Request {
        oid: Oid,
    },
    Run {
        node: NodeId,
        uuid: Uuid,
        log: Url,
    },
    Finished {
        node: NodeId,
        uuid: Uuid,
        reason: Reason,
    },
}

impl CobAction for Action {
    fn parents(&self) -> Vec<radicle::git::Oid> {
        Vec::new()
    }
}

impl Job {
    pub fn new(oid: Oid) -> Self {
        Self {
            oid,
            runs: HashMap::new(),
        }
    }

    pub fn oid(&self) -> &Oid {
        &self.oid
    }

    pub fn started(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.started())
    }

    pub fn finished(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.finished())
    }

    pub fn succeeded(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.succeeded())
    }

    pub fn failed(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.failed())
    }

    pub fn partition(&self) -> HashMap<NodeId, (Runs, Runs, Runs)> {
        self.runs
            .iter()
            .map(|(node, runs)| (*node, runs.partition()))
            .collect()
    }

    pub fn latest_of(&self, node: &NodeId) -> Option<(&Uuid, &Run)> {
        self.runs
            .get(node)
            .and_then(|runs| runs.0.iter().next_back())
    }

    pub fn latest(&self) -> impl Iterator<Item = (&NodeId, &Uuid, &Run)> + '_ {
        self.runs
            .iter()
            .filter_map(|(node, runs)| runs.latest().map(|(uuid, run)| (node, uuid, run)))
    }

    pub fn runs(&self) -> &HashMap<NodeId, Runs> {
        &self.runs
    }

    pub fn runs_of(&self, node: &NodeId) -> Option<&Runs> {
        self.runs.get(node)
    }

    fn filter_map_by<P>(&self, p: P) -> HashMap<NodeId, Runs>
    where
        P: Fn(&Runs) -> Runs,
    {
        self.runs
            .iter()
            .filter_map(|(node, runs)| {
                let runs = p(runs);
                (!runs.is_empty()).then_some((*node, runs))
            })
            .collect()
    }

    fn insert(&mut self, node: NodeId, uuid: Uuid, run: Run) -> bool {
        let runs = self.runs.entry(node).or_default();
        if runs.contains_key(&uuid) {
            false
        } else {
            runs.insert(uuid, run);
            true
        }
    }

    fn update(&mut self, node: NodeId, uuid: Uuid, reason: Reason) -> bool {
        let Some(runs) = self.runs.get_mut(&node) else {
            return false;
        };
        let mut updated = false;
        runs.0.entry(uuid).and_modify(|run| {
            updated = true;
            *run = run.clone().finish(reason);
        });
        updated
    }

    fn action(&mut self, action: Action) -> Result<(), error::Build> {
        match action {
            // Cannot request for another `oid`, so we ignore any superfluous
            // request actions
            Action::Request { .. } => Ok(()),
            Action::Run { node, uuid, log } => {
                self.insert(node, uuid, Run::new(log));
                Ok(())
            }
            Action::Finished { node, uuid, reason } => {
                self.update(node, uuid, reason);
                Ok(())
            }
        }
    }
}

impl store::Cob for Job {
    type Action = Action;
    type Error = error::Build;

    fn type_name() -> &'static TypeName {
        &TYPENAME
    }

    fn from_root<R: ReadRepository>(op: Op<Self::Action>, repo: &R) -> Result<Self, Self::Error> {
        let mut actions = op.actions.into_iter();
        let Some(Action::Request { oid }) = actions.next() else {
            return Err(error::Build::Initial);
        };
        repo.commit(oid)
            .map_err(|err| error::Build::MissingCommit { oid, err })?;
        let mut runs = Self::new(oid);
        for action in actions {
            runs.action(action)?;
        }
        Ok(runs)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a radicle::cob::Entry>>(
        &mut self,
        op: Op<Self::Action>,
        _concurrent: I,
        _repo: &R,
    ) -> Result<(), Self::Error> {
        for action in op.actions {
            self.action(action)?;
        }
        Ok(())
    }
}

impl<R: ReadRepository> Evaluate<R> for Job {
    type Error = error::Apply;

    fn init(entry: &radicle::cob::Entry, store: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let object = Job::from_root(op, store)?;
        Ok(object)
    }

    fn apply<'a, I: Iterator<Item = (&'a Oid, &'a radicle::cob::Entry)>>(
        &mut self,
        entry: &radicle::cob::Entry,
        concurrent: I,
        store: &R,
    ) -> Result<(), Self::Error> {
        let op = Op::try_from(entry)?;
        self.op(op, concurrent.map(|(_, e)| e), store)
            .map_err(error::Apply::from)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Run {
    status: Status,
    log: Url,
}

impl Run {
    pub fn new(log: Url) -> Self {
        Self {
            status: Status::Started,
            log,
        }
    }

    pub fn finish(self, reason: Reason) -> Self {
        Self {
            status: Status::Finished(reason),
            log: self.log,
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn is_started(&self) -> bool {
        match self.status {
            Status::Started => true,
            Status::Finished(_) => false,
        }
    }

    pub fn is_finished(&self) -> bool {
        !self.is_started()
    }

    pub fn succeeded(&self) -> bool {
        match self.status {
            Status::Started => false,
            Status::Finished(Reason::Failed) => false,
            Status::Finished(Reason::Succeeded) => true,
        }
    }

    pub fn failed(&self) -> bool {
        match self.status {
            Status::Started => false,
            Status::Finished(Reason::Failed) => true,
            Status::Finished(Reason::Succeeded) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Started,
    Finished(Reason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Reason {
    Failed,
    Succeeded,
}

pub struct Jobs<'a, R> {
    raw: store::Store<'a, Job, R>,
}

impl<'a, R> Deref for Jobs<'a, R> {
    type Target = store::Store<'a, Job, R>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a, R> Jobs<'a, R>
where
    R: ReadRepository + cob::Store,
{
    /// Open a jobs store.
    pub fn open(repository: &'a R) -> Result<Self, RepositoryError> {
        let identity = repository.identity_head()?;
        let raw = store::Store::open(repository)?.identity(identity);

        Ok(Self { raw })
    }

    /// Return the number of [`Job`]s in the store.
    pub fn counts(&self) -> Result<usize, store::Error> {
        Ok(self.all()?.count())
    }

    /// Get a [`Job`].
    pub fn get(&self, id: &ObjectId) -> Result<Option<Job>, store::Error> {
        self.raw.get(id)
    }
}

impl<'a, R> Jobs<'a, R>
where
    R: ReadRepository + SignRepository + cob::Store,
{
    /// Get a [`JobMut`].
    pub fn get_mut<'g, C>(&'g mut self, id: &ObjectId) -> Result<JobMut<'a, 'g, R>, store::Error> {
        let job = self
            .raw
            .get(id)?
            .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

        Ok(JobMut {
            id: *id,
            job,
            store: self,
        })
    }

    pub fn create<'g, G>(
        &'g mut self,
        oid: Oid,
        signer: &G,
    ) -> Result<JobMut<'a, 'g, R>, store::Error>
    where
        G: Signer,
    {
        let (id, job) = store::Transaction::initial::<_, _, Transaction<R>>(
            "Request job",
            &mut self.raw,
            signer,
            |tx, _| {
                tx.request(oid)?;
                Ok(())
            },
        )?;

        Ok(JobMut {
            id,
            job,
            store: self,
        })
    }
}

pub struct JobMut<'a, 'g, R> {
    pub id: ObjectId,

    job: Job,
    store: &'g mut Jobs<'a, R>,
}

impl<'a, 'g, R> Deref for JobMut<'a, 'g, R> {
    type Target = Job;

    fn deref(&self) -> &Self::Target {
        &self.job
    }
}

impl<'a, 'g, R> JobMut<'a, 'g, R>
where
    R: WriteRepository + cob::Store,
{
    pub fn new(id: ObjectId, job: Job, store: &'g mut Jobs<'a, R>) -> Self {
        Self { id, job, store }
    }

    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Reload the patch data from storage.
    pub fn reload(&mut self) -> Result<(), store::Error> {
        self.job = self
            .store
            .get(&self.id)?
            .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), self.id))?;

        Ok(())
    }

    pub fn request<G>(&mut self, oid: Oid, signer: &G) -> Result<EntryId, store::Error>
    where
        G: Signer,
    {
        self.transaction("Request OID", signer, |tx| tx.request(oid))
    }

    pub fn run<G>(
        &mut self,
        node: NodeId,
        uuid: Uuid,
        log: Url,
        signer: &G,
    ) -> Result<EntryId, store::Error>
    where
        G: Signer,
    {
        self.transaction("Run node job", signer, |tx| tx.run(node, uuid, log))
    }

    pub fn finish<G>(
        &mut self,
        node: NodeId,
        uuid: Uuid,
        reason: Reason,
        signer: &G,
    ) -> Result<EntryId, store::Error>
    where
        G: Signer,
    {
        self.transaction("Finished node job", signer, |tx| {
            tx.finish(node, uuid, reason)
        })
    }

    pub fn transaction<G, F>(
        &mut self,
        message: &str,
        signer: &G,
        operations: F,
    ) -> Result<EntryId, store::Error>
    where
        G: Signer,
        F: FnOnce(&mut Transaction<R>) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::default();
        operations(&mut tx)?;

        let (job, commit) = tx.0.commit(message, self.id, &mut self.store.raw, signer)?;
        self.job = job;

        Ok(commit)
    }
}

pub struct Transaction<R: ReadRepository>(store::Transaction<Job, R>);

impl<R> From<store::Transaction<Job, R>> for Transaction<R>
where
    R: ReadRepository,
{
    fn from(tx: store::Transaction<Job, R>) -> Self {
        Self(tx)
    }
}

impl<R> From<Transaction<R>> for store::Transaction<Job, R>
where
    R: ReadRepository,
{
    fn from(Transaction(tx): Transaction<R>) -> Self {
        tx
    }
}

impl<R> Default for Transaction<R>
where
    R: ReadRepository,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<R> Deref for Transaction<R>
where
    R: ReadRepository,
{
    type Target = store::Transaction<Job, R>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R> DerefMut for Transaction<R>
where
    R: ReadRepository,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<R> Transaction<R>
where
    R: ReadRepository,
{
    pub fn request(&mut self, oid: Oid) -> Result<(), store::Error> {
        self.0.push(Action::Request { oid })
    }

    pub fn run(&mut self, node: NodeId, uuid: Uuid, log: Url) -> Result<(), store::Error> {
        self.0.push(Action::Run { node, uuid, log })
    }

    pub fn finish(&mut self, node: NodeId, uuid: Uuid, reason: Reason) -> Result<(), store::Error> {
        self.0.push(Action::Finished { node, uuid, reason })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use radicle::crypto::Signer;
    use radicle::git::{raw::Repository, Oid};
    use radicle::test;
    use url::Url;
    use uuid::Uuid;

    use crate::{Jobs, Reason, Run, Runs};

    fn node_run() -> (Uuid, Url) {
        let uuid = Uuid::new_v4();
        let log = Url::parse(&format!("https://example.com/ci/logs?run={}", uuid)).unwrap();
        (uuid, log)
    }

    fn commit(repo: &Repository) -> Oid {
        let tree = {
            let tree = repo.treebuilder(None).unwrap();
            let oid = tree.write().unwrap();
            repo.find_tree(oid).unwrap()
        };

        let author = repo.signature().unwrap();
        repo.commit(None, &author, &author, "Test Commit", &tree, &[])
            .unwrap()
            .into()
    }

    #[test]
    fn e2e() {
        let test::setup::NodeWithRepo {
            node: alice, repo, ..
        } = test::setup::NodeWithRepo::default();
        let oid = commit(&repo.backend);
        let mut jobs = Jobs::open(&*repo).unwrap();

        let test::setup::NodeWithRepo { node: bob, .. } = test::setup::NodeWithRepo::default();
        let mut job = jobs.create(oid, &alice.signer).unwrap();

        let (alice_uuid, alice_log) = node_run();
        job.run(
            *alice.signer.public_key(),
            alice_uuid,
            alice_log.clone(),
            &alice.signer,
        )
        .unwrap();

        let (bob_uuid, bob_log) = node_run();
        job.run(
            *bob.signer.public_key(),
            bob_uuid,
            bob_log.clone(),
            &bob.signer,
        )
        .unwrap();

        let alice_runs = job.runs_of(alice.signer.public_key());
        assert!(alice_runs.is_some());
        assert_eq!(
            *alice_runs.unwrap(),
            [(alice_uuid, Run::new(alice_log))]
                .into_iter()
                .collect::<Runs>()
        );

        let bob_runs = job.runs_of(bob.signer.public_key());
        assert!(bob_runs.is_some());
        assert_eq!(
            *bob_runs.unwrap(),
            [(bob_uuid, Run::new(bob_log))]
                .into_iter()
                .collect::<Runs>()
        );

        job.finish(
            *alice.signer.public_key(),
            alice_uuid,
            Reason::Succeeded,
            &alice.signer,
        )
        .unwrap();

        let finished = job.finished();
        assert!(finished.contains_key(alice.signer.public_key()));
        assert!(!finished.contains_key(bob.signer.public_key()));

        job.finish(
            *bob.signer.public_key(),
            bob_uuid,
            Reason::Failed,
            &bob.signer,
        )
        .unwrap();

        let succeeded = job.succeeded();
        assert!(succeeded.contains_key(alice.signer.public_key()));
        assert!(!succeeded.contains_key(bob.signer.public_key()));
        let failed = job.failed();
        assert!(!failed.contains_key(alice.signer.public_key()));
        assert!(failed.contains_key(bob.signer.public_key()));
        let started = job.started();
        assert!(started.is_empty());
    }

    #[test]
    fn missing_commit() {
        let test::setup::NodeWithRepo {
            node: alice, repo, ..
        } = test::setup::NodeWithRepo::default();
        let mut jobs = Jobs::open(&*repo).unwrap();
        let oid = test::arbitrary::oid();
        let job = jobs.create(oid, &alice.signer);
        assert!(job.is_err())
    }

    #[test]
    fn idempotent_create() {
        let test::setup::NodeWithRepo {
            node: alice, repo, ..
        } = test::setup::NodeWithRepo::default();
        let oid = commit(&repo.backend);
        let mut jobs = Jobs::open(&*repo).unwrap();
        let job1 = {
            let job1 = jobs.create(oid, &alice.signer).unwrap();
            job1.id
        };
        let job2 = {
            let job2 = jobs.create(oid, &alice.signer).unwrap();
            job2.id
        };

        assert_eq!(job1, job2);
        assert_eq!(jobs.get(&job1).unwrap(), jobs.get(&job2).unwrap());
    }
}
