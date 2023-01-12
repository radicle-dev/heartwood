use core::time;
use std::io::prelude::*;
use std::thread::JoinHandle;
use std::{env, io, net, process, thread};

use crossbeam_channel as chan;
use netservices::resources::SplitIo;
use netservices::tunnel::Tunnel;
use netservices::NetSession;

use radicle::identity::Id;
use radicle::storage::{ReadRepository, RefUpdate, WriteRepository, WriteStorage};
use radicle::Storage;
use reactor::poller::popol;

use crate::service::reactor::Fetch;
use crate::service::{FetchError, FetchResult};
use crate::wire::{Noise, NoiseReader, NoiseWriter};

/// Worker request.
pub struct WorkerReq {
    pub fetch: Fetch,
    pub session: Noise,
    pub drain: Vec<u8>,
    pub channel: chan::Sender<WorkerResp>,
}

/// Worker response.
pub struct WorkerResp {
    pub result: FetchResult,
    pub session: Noise,
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
        mut session: Noise,
    ) -> (Noise, Result<Vec<RefUpdate>, FetchError>) {
        if fetch.initiated {
            log::debug!(target: "worker", "Worker processing outgoing fetch for {}", fetch.repo);

            let mut tunnel = match Tunnel::with(session, net::SocketAddr::from(([0, 0, 0, 0], 0))) {
                Ok(tunnel) => tunnel,
                Err((session, err)) => return (session, Err(err.into())),
            };
            let result = self.fetch(fetch, &mut tunnel);
            let session = tunnel.into_session();

            (session, result)
        } else {
            log::debug!(target: "worker", "Worker processing incoming fetch for {}", fetch.repo);

            if let Err(err) = session.set_nonblocking(false) {
                return (session, Err(err.into()));
            }
            let (mut stream_r, mut stream_w) = match session.split_io() {
                Ok((r, w)) => (r, w),
                Err(err) => {
                    return (err.original, Err(err.error.into()));
                }
            };
            let result = self.upload_pack(fetch, drain, &mut stream_r, &mut stream_w);
            let session = Noise::from_split_io(stream_r, stream_w);

            (session, result)
        }
    }

    fn fetch(
        &self,
        fetch: &Fetch,
        tunnel: &mut Tunnel<Noise>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let repo = self.storage.repository(fetch.repo)?;
        let tunnel_addr = tunnel.local_addr()?;
        let mut cmd = process::Command::new("git");
        cmd.current_dir(repo.path())
            .env("GIT_PROTOCOL", "2")
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("GIT_TRACE")))
            .arg("fetch")
            .arg("--atomic")
            .arg("--verbose")
            .arg(format!("git://{tunnel_addr}/{}", repo.id))
            .arg(fetch.namespaces.as_fetchspec())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .stdin(process::Stdio::piped());

        log::debug!(target: "worker", "Running command: {:?}", cmd);

        let mut child = cmd.spawn()?;
        let mut stderr = child.stderr.take().unwrap();

        let _ = tunnel.tunnel_once(popol::Poller::new(), self.timeout)?;
        let status = child.wait()?;

        // TODO: Parse fetch output to return updates.
        log::debug!(target: "worker", "Fetch for {} exited with status {:?}", fetch.repo, status.code());

        if let Some(status) = status.code() {
            log::debug!(target: "worker", "Upload pack for {} exited with status {:?}", fetch.repo, status);
        } else {
            log::debug!(target: "worker", "Upload pack for {} exited with unknown status", fetch.repo);
        }

        if !status.success() {
            let mut err = Vec::new();
            stderr.read_to_end(&mut err)?;

            let err = String::from_utf8_lossy(&err);
            log::debug!(target: "worker", "Fetch for {}: stderr: {err}", fetch.repo);
        }
        let head = repo.set_head()?;
        log::debug!(target: "worker", "Setting head for {} to {head}", fetch.repo);

        Ok(vec![])
    }

    fn upload_pack(
        &self,
        fetch: &Fetch,
        drain: Vec<u8>,
        stream_r: &mut NoiseReader,
        stream_w: &mut NoiseWriter,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let repo = self.storage.repository(fetch.repo)?;
        let mut child = process::Command::new("git")
            .current_dir(repo.path())
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("GIT_TRACE")))
            .env("GIT_PROTOCOL", "2")
            .arg("upload-pack")
            .arg("--strict") // The path to the git repo must be exact.
            .arg(".")
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .stdin(process::Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

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
                    if let Ok(line) = std::str::from_utf8(&buf[..n]) {
                        // FIXME: The git command could come in the drain object.
                        // FIXME: We should only call this once, before looping.
                        if let Some(cmd) = GitCommand::parse(line) {
                            // FIXME: Convert this into an error.
                            debug_assert_eq!(cmd.repo, fetch.repo);
                            continue;
                        }
                    }
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
        let status = child.wait()?;

        if let Some(status) = status.code() {
            log::debug!(target: "worker", "Upload pack for {} exited with status {:?}", fetch.repo, status);
        } else {
            log::debug!(target: "worker", "Upload pack for {} exited with unknown status", fetch.repo);
        }

        if !status.success() {
            let mut err = Vec::new();
            stderr.read_to_end(&mut err)?;

            let err = String::from_utf8_lossy(&err);
            log::debug!(target: "worker", "Upload pack for {}: stderr: {}", fetch.repo, err);
        }

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
        name: String,
    ) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            let worker = Worker {
                tasks: tasks.clone(),
                storage: storage.clone(),
                timeout,
            };
            let thread = thread::Builder::new()
                .name(name.clone())
                .spawn(|| worker.run())
                .unwrap();

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

#[derive(Debug)]
pub struct GitCommand {
    pub repo: Id,
    pub path: String,
    pub host: Option<(String, Option<u16>)>,
    pub extra: Vec<(String, Option<String>)>,
}

impl GitCommand {
    /// Parse a Git command packet-line.
    ///
    /// Example: `0032git-upload-pack /project.git\0host=myserver.com\0`
    ///
    fn parse(input: &str) -> Option<Self> {
        let (left, right) = input.split_at(4);
        let len = usize::from_str_radix(left, 16).ok()?;
        if len != input.len() {
            return None;
        }
        let mut parts = right
            .strip_prefix("git-upload-pack ")?
            .split_terminator('\0');

        let path = parts.next()?.to_owned();
        let repo = path.strip_prefix('/')?.parse().ok()?;
        let host = match parts.next() {
            None | Some("") => None,
            Some(host) => {
                let host = host.strip_prefix("host=")?;
                match host.split_once(':') {
                    None => Some((host.to_owned(), None)),
                    Some((host, port)) => {
                        let port = port.parse::<u16>().ok()?;
                        Some((host.to_owned(), Some(port)))
                    }
                }
            }
        };
        let extra = parts
            .skip_while(|part| part.is_empty())
            .map(|part| match part.split_once('=') {
                None => (part.to_owned(), None),
                Some((k, v)) => (k.to_owned(), Some(v.to_owned())),
            })
            .collect();

        Some(Self {
            repo,
            path,
            host,
            extra,
        })
    }
}
