use crossbeam_channel as chan;
use netservices::noise::NoiseXk;
use std::collections::VecDeque;
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
    task_send: chan::Sender<WorkerReq<G>>,
    free_recv: chan::Receiver<()>,
    thread: JoinHandle<()>,
}

impl<G: Negotiator> Worker<G> {
    fn new(storage: Storage) -> Self
    where
        G: 'static,
    {
        let (task_send, task_recv) = chan::unbounded::<WorkerReq<G>>();
        let (free_send, free_recv) = chan::bounded::<()>(1);

        let runtime = WorkerRuntime {
            task_recv,
            free_send,
            storage,
        };

        let thread = thread::spawn(|| runtime.run());

        Self {
            task_send,
            free_recv,
            thread,
        }
    }

    pub fn delegate(&self, task: WorkerReq<G>) -> Result<(), chan::SendError<WorkerReq<G>>> {
        self.task_send.send(task)
    }
}

pub struct WorkerRuntime<G: Negotiator> {
    storage: Storage,
    task_recv: chan::Receiver<WorkerReq<G>>,
    free_send: chan::Sender<()>,
}

impl<G: Negotiator> WorkerRuntime<G> {
    pub fn run(self) {
        loop {
            let task = self.task_recv.recv().expect("worker channel is broken");
            self.process(task);
            self.free_send.send(()).expect("worker channel is broken")
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

pub struct WorkerPool<G: Negotiator> {
    pool: Vec<Worker<G>>,
    free_workers: VecDeque<usize>,
    queue: VecDeque<WorkerReq<G>>,
}

impl<G: Negotiator> WorkerPool<G> {
    pub fn with(storage: Storage, capacity: usize) -> Self
    where
        G: 'static,
    {
        let mut pool = Vec::with_capacity(capacity);
        let mut free_workers = VecDeque::with_capacity(capacity);
        for index in 0..capacity {
            let worker = Worker::new(storage.clone());
            pool.push(worker);
            free_workers.push_back(index);
        }
        Self {
            pool,
            free_workers,
            queue: Default::default(),
        }
    }

    pub fn join(mut self, recv_task: chan::Receiver<WorkerReq<G>>) {
        let mut sel = chan::Select::new();
        let mut recv = Vec::with_capacity(self.pool.len());

        sel.recv(&recv_task);
        for worker in &self.pool {
            recv.push(worker.free_recv.clone());
        }
        for r in &recv {
            sel.recv(r);
        }

        loop {
            let oper = sel.select();
            let index = oper.index();
            if index == 0 {
                let mut task = oper
                    .recv(&recv_task)
                    .expect("broken worker request channel");

                loop {
                    let next = match self.free_workers.pop_front() {
                        Some(next) => next,
                        None => {
                            self.queue.push_back(task);
                            break;
                        }
                    };
                    sel.remove(next + 1);
                    let worker = &self.pool[next];
                    match worker.delegate(task) {
                        Ok(_) => return,
                        Err(err) => task = err.0,
                    }
                }
            } else {
                let _ = oper.recv(&recv[index - 1]);
                if let Some(task) = self.queue.pop_front() {
                    if let Err(err) = self.pool[index - 1].delegate(task) {
                        self.queue.push_front(err.0);
                    }
                    continue;
                }
                sel.recv(&recv[index - 1]);
                self.free_workers.push_back(index - 1);
            }
        }
    }
}
