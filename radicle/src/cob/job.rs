//! Track "jobs" related to a repository.
//!
//! The purpose of this COB is to allow users of Radicle to have a way
//! of keeping track of what automated processing of changes to a
//! repository have been done. A "job" might be a continuous
//! integration automation building the software in a repository and
//! running its automated tests. A delegate for the repository could
//! track COBs emitted by trusted nodes to help with deciding when a
//! patch is ready for them to merge.

use std::{ops::Deref, str::FromStr};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::cob;
use crate::cob::change::store::Entry;
use crate::cob::store;
use crate::cob::store::{Cob, CobAction, Store, Transaction};
use crate::cob::{EntryId, ObjectId, TypeName};
use crate::crypto::ssh::ExtendedSignature;
use crate::git;
use crate::node::device::Device;
use crate::prelude::ReadRepository;
use crate::storage::{Oid, WriteRepository};

use super::store::CobWithType;

/// The name of this COB type. Note that this is a "beta" COB, which
/// means it's not meant for others to rely on yet, and we may change
/// it without warning.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.beta.job").expect("type name is valid"));

/// An identifier for the job.
pub type JobId = ObjectId;

/// All the possible errors from this type of COB.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("initialization failed: {0}")]
    Init(&'static str),
    #[error("op decoding failed: {0}")]
    Op(#[from] cob::op::OpEncodingError),
    #[error("store: {0}")]
    Store(#[from] store::Error),
    #[error("can't trigger a job which is not fresh")]
    TriggerWhenNotFresh,
    #[error("can't start a job which is not triggered")]
    StartWhenNotFresh,
    #[error("can't finish a job which is not running")]
    FinishWhenNotRunning,
}

/// All the possible states of this COB.
///
/// This is primarily modeled for CI, for now. A COB is created by a
/// node when it triggers a CI run, and then updated when the CI
/// system actually starts executing the run, and finishes. The CI
/// system assigns an identifier to the run, and may have a URL for
/// the log. These can also be stored in the COB.
///
/// This COB is essentially a state machine that tracks the state of
/// an automated run Ci. When the COB is created, in state `Fresh`, it
/// just records the git commit the run uses. This can't be changed.
/// The COB may be created before the run actually starts. Once the
/// run starts, the COB state changes to `Running`. When the run
/// finished, the state changes to `Finished`, and the state records
/// why the run finished: `Succeeded` or `Failed`.
///
/// No other state changes are allowed for the COB.
///
/// Note that if CI runs again for the same commit, a new COB is
/// created. The two runs may result in different outcomes, even if
/// nothing in the source code has changed. For example, the CI system
/// may run out of disk space, or use different versions of the
/// software used in the run.
#[derive(Debug, Default, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum State {
    /// COB has been created, job has not yet started running.
    #[default]
    Fresh,
    /// Job has started running.
    Running,
    /// Job has finished.
    Finished(Reason),
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fresh => write!(f, "fresh"),
            Self::Running => write!(f, "running"),
            Self::Finished(Reason::Succeeded) => write!(f, "succeeded"),
            Self::Finished(Reason::Failed) => write!(f, "failed"),
        }
    }
}

/// Why did build finish?
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    /// Build was successful.
    Succeeded,
    /// Build failed for some reason.
    Failed,
}

/// Actions to update this COB.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Initialize the COB for a new job.
    Trigger { commit: git::Oid },

    /// Start a job.
    Start {
        run_id: String,
        info_url: Option<String>,
    },

    /// Finish a job.
    Finish { reason: Reason },
}

impl CobAction for Action {}

/// Type of COB operation.
pub type Op = cob::Op<Action>;

/// The COB with actions applied.
///
/// A job is based on a specific commit. This is set when the COB is
/// created and can't be changed.
///
/// A job has a specific [`State`].
///
/// A job may have a "run id", which is an arbitrary string. It might
/// be the identifier for a CI run, set by an external CI system, for
/// example. The id is informational and there no guarantees what it
/// means, or that it's unique.
///
/// A job may also store a URL to more information. This might be a
/// link to a run log in a CI system, for example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    commit: git::Oid,
    state: State,
    run_id: Option<String>,
    info_url: Option<String>,
}

impl Job {
    /// Create a new `Job` in the `Fresh` state, using the provided `commit`.
    fn new(commit: git::Oid) -> Self {
        Self {
            commit,
            state: State::default(),
            run_id: None,
            info_url: None,
        }
    }

    /// Get the commit that this `Job` was created with.
    pub fn commit(&self) -> git::Oid {
        self.commit
    }

    /// Get the run identifier, if any, that was associated with this `Job`.
    pub fn run_id(&self) -> Option<&str> {
        self.run_id.as_deref()
    }

    /// Get the info URL, if any, that was associated with this `Job`.
    pub fn info_url(&self) -> Option<&str> {
        self.info_url.as_deref()
    }

    /// Get the `State` of this `Job`.
    pub fn state(&self) -> State {
        self.state
    }

    /// Apply a single action to the job.
    fn action(&mut self, action: Action) -> Result<(), Error> {
        match action {
            Action::Trigger { .. } => {
                if self.state != State::Fresh {
                    return Err(Error::TriggerWhenNotFresh);
                }
            }

            Action::Start { run_id, info_url } => {
                if self.state != State::Fresh {
                    return Err(Error::StartWhenNotFresh);
                }
                self.state = State::Running;
                self.run_id = Some(run_id);
                self.info_url = info_url;
            }

            Action::Finish { reason } => {
                if self.state != State::Running {
                    return Err(Error::FinishWhenNotRunning);
                }
                self.state = State::Finished(reason);
            }
        }
        Ok(())
    }
}

impl Cob for Job {
    type Action = Action;
    type Error = Error;

    fn from_root<R: ReadRepository>(op: Op, _repo: &R) -> Result<Self, Self::Error> {
        let mut actions = op.actions.into_iter();
        let Some(Action::Trigger { commit }) = actions.next() else {
            return Err(Error::Init("the first action must be of type `trigger`"));
        };
        let mut job = Job::new(commit);

        for action in actions {
            job.action(action)?;
        }
        Ok(job)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a cob::Entry>>(
        &mut self,
        op: Op,
        _concurrent: I,
        _repo: &R,
    ) -> Result<(), Error> {
        // Some day this needs to check authorization. However, we
        // don't yet know what the rules should be.
        for action in op.actions {
            self.action(action)?;
        }
        Ok(())
    }
}

impl CobWithType for Job {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }
}

impl<R: ReadRepository> cob::Evaluate<R> for Job {
    type Error = Error;

    fn init(entry: &cob::Entry, repo: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let job = Job::from_root(op, repo)?;
        Ok(job)
    }

    fn apply<'a, I>(
        &mut self,
        entry: &cob::Entry,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (&'a Oid, &'a Entry<Oid, Oid, ExtendedSignature>)>,
    {
        let op = Op::try_from(entry)?;
        self.op(op, concurrent.map(|(_, e)| e), repo)
    }
}

impl<R: ReadRepository> Transaction<Job, R> {
    /// Push an [`Action::Trigger`] which will create a new `Job` with the
    /// provided `commit` in the [`State::Fresh`] state.
    pub fn trigger(&mut self, commit: git::Oid) -> Result<(), store::Error> {
        self.push(Action::Trigger { commit })
    }

    /// Push an [`Action::Start`] which will start the `Job` with the provided
    /// metadata and move the `Job` into the [`State::Running`] state.
    pub fn start(&mut self, run_id: String, info_url: Option<String>) -> Result<(), store::Error> {
        self.push(Action::Start { run_id, info_url })
    }

    /// Push an [`Action::Finish`] which will finish the `Job` with the provided
    /// reason, moving the `Job` into the [`State::Finished`] state.
    pub fn finish(&mut self, reason: Reason) -> Result<(), store::Error> {
        self.push(Action::Finish { reason })
    }
}

pub struct JobMut<'a, 'g, R> {
    id: ObjectId,
    job: Job,
    store: &'g mut JobStore<'a, R>,
}

impl<'a, 'g, R> From<JobMut<'a, 'g, R>> for (JobId, Job) {
    fn from(value: JobMut<'a, 'g, R>) -> Self {
        (value.id, value.job)
    }
}

impl<R> std::fmt::Debug for JobMut<'_, '_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("JobMut")
            .field("id", &self.id)
            .field("job", &self.job)
            .finish()
    }
}

impl<R> JobMut<'_, '_, R>
where
    R: WriteRepository + cob::Store,
{
    /// Reload the COB from storage.
    pub fn reload(&mut self) -> Result<(), store::Error> {
        self.job = self
            .store
            .get(&self.id)?
            .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), self.id))?;
        Ok(())
    }

    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Transition the `Job` into a running state, storing the provided
    /// metadata.
    pub fn start<G>(
        &mut self,
        run_id: String,
        info_url: Option<String>,
        signer: &Device<G>,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Start", signer, |tx| {
            tx.start(run_id, info_url)?;
            Ok(())
        })
    }

    /// Transition the `Job` into a finished state, with the provided `reason`.
    pub fn finish<G>(&mut self, reason: Reason, signer: &Device<G>) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.transaction("Finish", signer, |tx| tx.finish(reason))
    }

    pub fn transaction<G, F>(
        &mut self,
        message: &str,
        signer: &Device<G>,
        operations: F,
    ) -> Result<EntryId, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
        F: FnOnce(&mut Transaction<Job, R>) -> Result<(), store::Error>,
    {
        let mut tx = Transaction::default();
        operations(&mut tx)?;

        let (job, id) = tx.commit(message, self.id, &mut self.store.raw, signer)?;
        self.job = job;

        Ok(id)
    }
}

impl<R> Deref for JobMut<'_, '_, R> {
    type Target = Job;

    fn deref(&self) -> &Self::Target {
        &self.job
    }
}

pub struct JobStore<'a, R> {
    raw: Store<'a, Job, R>,
}

impl<'a, R> Deref for JobStore<'a, R> {
    type Target = Store<'a, Job, R>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a, R> JobStore<'a, R>
where
    R: WriteRepository + ReadRepository + cob::Store,
{
    pub fn open(repository: &'a R) -> Result<Self, store::Error> {
        let raw = store::Store::open(repository)?;
        Ok(Self { raw })
    }

    /// Get the `Job`, if any, identified by `id`.
    pub fn get(&self, id: &JobId) -> Result<Option<Job>, store::Error> {
        self.raw.get(id)
    }

    /// Get the `Job`, identified by `id`, which can be mutated.
    ///
    /// # Errors
    ///
    /// This will fail if the `Job` could not be found.
    pub fn get_mut<'g>(&'g mut self, id: &JobId) -> Result<JobMut<'a, 'g, R>, store::Error> {
        let job = self
            .raw
            .get(id)?
            .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), *id))?;
        Ok(JobMut {
            id: *id,
            job,
            store: self,
        })
    }

    /// Create a fresh `Job` with the provided `commit_id`.
    pub fn create<'g, G>(
        &'g mut self,
        commit_id: git::Oid,
        signer: &Device<G>,
    ) -> Result<JobMut<'a, 'g, R>, Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        let (id, job) = Transaction::initial("Create job", &mut self.raw, signer, |tx, _| {
            tx.trigger(commit_id)?;
            Ok(())
        })?;

        Ok(JobMut {
            id,
            job,
            store: self,
        })
    }

    /// Delete the `Job` identified by `id`.
    pub fn remove<G>(&self, id: &JobId, signer: &Device<G>) -> Result<(), store::Error>
    where
        G: crypto::signature::Signer<crypto::Signature>,
    {
        self.raw.remove(id, signer)
    }
}
