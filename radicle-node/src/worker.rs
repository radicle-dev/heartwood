use crossbeam_channel as chan;
use netservices::noise::NoiseXk;
use std::thread;
use std::thread::JoinHandle;

use radicle::crypto::Negotiator;
use radicle::storage::WriteStorage;
use radicle::Storage;

use crate::service::reactor::Fetch;
use crate::service::FetchResult;

pub struct WorkerReq<G: Negotiator> {
    pub fetch: Fetch,
    pub session: NoiseXk<G>,
    pub drain: Vec<u8>,
    pub channel: chan::Sender<WorkerResp<G>>,
}

pub struct WorkerResp<G: Negotiator> {
    pub result: FetchResult,
    pub session: NoiseXk<G>,
}

pub struct Worker<G: Negotiator> {
    storage: Storage,
    tasks: chan::Receiver<WorkerReq<G>>,
}

impl<G: Negotiator> Worker<G> {
    pub fn run(self) {
        loop {
            let task = self.tasks.recv().expect("worker task channel is broken");
            self.process(task);
        }
    }

    pub fn process(&self, task: WorkerReq<G>) {
        let WorkerReq {
            fetch,
            session,
            drain,
            channel,
        } = task;
        let result = match self.storage.repository(fetch.repo) {
            Ok(_) => FetchResult::Fetched {
                from: fetch.remote,
                updated: vec![],
            },
            Err(err) => FetchResult::Error {
                from: fetch.remote,
                error: err.into(),
            },
        };
        if channel.send(WorkerResp { result, session }).is_err() {
            log::error!("unable to report fetch result: the P2P reactor has closed the channel to the worker");
        }
        todo!("cloudhead: implement worker business logic");
    }
}

pub struct WorkerPool {
    pool: Vec<JoinHandle<()>>,
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

    pub fn join(self) -> thread::Result<()> {
        for worker in self.pool {
            worker.join()?;
        }
        Ok(())
    }
}
