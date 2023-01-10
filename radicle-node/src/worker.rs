use core::time;
use std::io::prelude::*;
use std::process;
use std::thread;
use std::thread::JoinHandle;
use std::{io, net};

use crossbeam_channel as chan;
use netservices::resources::{NetReader, NetResource, NetWriter, SplitIo};
use netservices::tunnel::Tunnel;

use radicle::storage::{ReadRepository, RefUpdate, WriteStorage};
use radicle::Storage;
use reactor::poller::popol;

use crate::service::reactor::Fetch;
use crate::service::{FetchError, FetchResult};
use crate::wire::Noise;

/// Worker request.
pub struct WorkerReq {
    pub fetch: Fetch,
    pub session: NetResource<Noise>,
    pub drain: Vec<u8>,
    pub channel: chan::Sender<WorkerResp>,
}

/// Worker response.
pub struct WorkerResp {
    pub result: FetchResult,
    pub session: NetResource<Noise>,
}

/// A worker that replicates git objects.
struct Worker {
    storage: Storage,
    tasks: chan::Receiver<WorkerReq>,
    timeout: time::Duration,
}

impl Worker {
    /// Waits for tasks and runs them. Blocks indefinitely unless there is an error receiving
    /// the next task.
    fn run(self) -> Result<(), chan::RecvError> {
        loop {
            let task = self.tasks.recv()?;
            self.process(task);
        }
    }

    fn process(&self, task: WorkerReq) {
        let WorkerReq {
            fetch,
            session,
            drain,
            channel,
        } = task;

        let (session, result) = self._process(&fetch, drain, session);
        let result = match result {
            Ok(updated) => FetchResult::Fetched {
                from: fetch.remote,
                updated,
            },
            Err(error) => FetchResult::Error {
                from: fetch.remote,
                error,
            },
        };
        if channel.send(WorkerResp { result, session }).is_err() {
            log::error!("Unable to report fetch result: worker channel disconnected");
        }
    }

    fn _process(
        &self,
        fetch: &Fetch,
        drain: Vec<u8>,
        session: NetResource<Noise>,
    ) -> (NetResource<Noise>, Result<Vec<RefUpdate>, FetchError>) {
        if fetch.initiated {
            let mut tunnel = match Tunnel::with(session, net::SocketAddr::from(([0, 0, 0, 0], 0))) {
                Ok(tunnel) => tunnel,
                Err((session, err)) => return (session, Err(err.into())),
            };
            let result = self.fetch(fetch, &mut tunnel);
            let session = tunnel.into_session();

            (session, result)
        } else {
            let (mut stream_r, mut stream_w) = match session.split_io() {
                Ok((r, w)) => (r, w),
                Err(err) => {
                    return (err.original, Err(err.error.into()));
                }
            };
            let result = self.upload_pack(fetch, drain, &mut stream_r, &mut stream_w);
            let session = NetResource::from_split_io(stream_r, stream_w);

            (session, result)
        }
    }

    fn fetch(
        &self,
        fetch: &Fetch,
        tunnel: &mut Tunnel<NetResource<Noise>>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let tunnel_addr = tunnel.local_addr()?;
        let repo = self.storage.repository(fetch.repo)?;
        let child = process::Command::new("git")
            .current_dir(repo.path())
            .arg("fetch")
            .arg("--atomic") // The path to the git repo must be exact.
            .arg(format!("git://{tunnel_addr}"))
            .arg(fetch.namespaces.as_fetchspec())
            .arg(".")
            .stdout(process::Stdio::piped())
            .stdin(process::Stdio::piped())
            .spawn()?;

        let _ = tunnel.tunnel_once(popol::Poller::new(), self.timeout)?;
        let output = child.wait_with_output()?;

        // TODO: Parse fetch output to return updates.
        log::debug!(target: "worker", "Fetch output for {}: {:?}", fetch.repo, output);

        Ok(vec![])
    }

    fn upload_pack(
        &self,
        fetch: &Fetch,
        drain: Vec<u8>,
        stream_r: &mut NetReader<Noise>,
        stream_w: &mut NetWriter<Noise>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let repo = self.storage.repository(fetch.repo)?;
        let mut child = process::Command::new("git")
            .current_dir(repo.path())
            .arg("upload-pack")
            .arg("--strict") // The path to the git repo must be exact.
            .arg(".")
            .stdout(process::Stdio::piped())
            .stdin(process::Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();

        thread::scope(|scope| {
            let t = scope.spawn(move || {
                let mut buf = [0u8; 65535];

                // First drain the buffer of incoming data that was waiting.
                if stdin.write_all(&drain[..]).is_err() {
                    return;
                }
                // Then process any new data coming into the socket, and write it
                // to the standard input of the `upload-pack` process.
                while let Ok(n) = stream_r.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if stdin.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
            // Output of `upload-pack` is sent back to the remote peer.
            io::copy(&mut stdout, stream_w)?;
            // SAFETY: The thread does not panic, unless the implementations of read/write
            // internally panic.
            t.join().unwrap();

            Ok::<_, FetchError>(())
        })?;
        child.wait()?;

        Ok(vec![])
    }
}

/// A pool of workers. One thread is allocated for each worker.
pub struct WorkerPool {
    pool: Vec<JoinHandle<Result<(), chan::RecvError>>>,
}

impl WorkerPool {
    /// Create a new worker pool with the given parameters.
    pub fn with(
        capacity: usize,
        timeout: time::Duration,
        storage: Storage,
        tasks: chan::Receiver<WorkerReq>,
    ) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            let worker = Worker {
                tasks: tasks.clone(),
                storage: storage.clone(),
                timeout,
            };
            let thread = thread::spawn(|| worker.run());

            pool.push(thread);
        }
        Self { pool }
    }

    /// Run the worker pool.
    ///
    /// Blocks until all worker threads have exited.
    pub fn run(self) -> thread::Result<()> {
        for worker in self.pool {
            if let Err(err) = worker.join()? {
                log::error!(target: "pool", "Worker failed: {err}");
            }
        }
        log::debug!(target: "pool", "Worker pool shutting down..");

        Ok(())
    }
}
