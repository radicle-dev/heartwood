use crossbeam_channel as chan;
use netservices::noise::NoiseXk;
use netservices::wire::NetTransport;
use std::thread;
use std::thread::JoinHandle;

use radicle::crypto::Negotiator;
use radicle::storage::WriteStorage;
use radicle::Storage;

use crate::service::reactor::Fetch;
use crate::service::FetchResult;

/// Worker request.
pub struct WorkerReq<G: Negotiator> {
    pub fetch: Fetch,
    pub session: NetTransport<NoiseXk<G>>,
    pub drain: Vec<u8>,
    pub channel: chan::Sender<WorkerResp<G>>,
}

/// Worker response.
pub struct WorkerResp<G: Negotiator> {
    pub result: FetchResult,
    pub session: NetTransport<NoiseXk<G>>,
}

pub struct Worker<G: Negotiator> {
    storage: Storage,
    tasks: chan::Receiver<WorkerReq<G>>,
}

impl<G: Negotiator> Worker<G> {
    pub fn run(self) -> Result<(), chan::RecvError> {
        loop {
            let task = self.tasks.recv()?;
            self.process(task);
        }
    }

    pub fn process(&self, task: WorkerReq<G>) {
        let WorkerReq {
            fetch,
            session,
            // TODO: Implement logic.
            drain: _drain,
            channel,
        } = task;
        let result = match self.storage.repository(fetch.repo) {
            Ok(_) => todo!(),
            Err(err) => FetchResult::Error {
                from: fetch.remote,
                error: err.into(),
            },
        };
        if channel.send(WorkerResp { result, session }).is_err() {
            log::error!("Unable to report fetch result: worker channel disconnected");
        }
    }
}

pub struct WorkerPool {
    pool: Vec<JoinHandle<Result<(), chan::RecvError>>>,
}

impl WorkerPool {
    pub fn with<G: Negotiator + 'static>(
        capacity: usize,
        storage: Storage,
        tasks: chan::Receiver<WorkerReq<G>>,
    ) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            let runtime = Worker {
                tasks: tasks.clone(),
                storage: storage.clone(),
            };
            let thread = thread::spawn(|| runtime.run());
            pool.push(thread);
        }
        Self { pool }
    }

    pub fn run(self) -> thread::Result<()> {
        for worker in self.pool {
            let result = worker.join()?;
            if let Err(err) = result {
                log::error!(target: "pool", "Worker failed: {err}");
            }
        }
        Ok(())
    }
}
