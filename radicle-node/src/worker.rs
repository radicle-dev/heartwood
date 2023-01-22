use std::io::prelude::*;
use std::thread::JoinHandle;
use std::{env, io, net, process, str, thread, time};

use crossbeam_channel as chan;
use cyphernet::EcSign;
use netservices::tunnel::Tunnel;
use netservices::{NetSession, SplitIo};

use radicle::crypto::Signer;
use radicle::identity::Id;
use radicle::storage::{ReadRepository, RefUpdate, WriteRepository, WriteStorage};
use radicle::{git, Storage};
use reactor::poller::popol;

use crate::client::handle::Handle;
use crate::node::{FetchError, FetchResult};
use crate::service::reactor::Fetch;
use crate::wire::{WireReader, WireSession, WireWriter};

/// Worker request.
pub struct WorkerReq<G: Signer + EcSign> {
    pub fetch: Fetch,
    pub session: WireSession<G>,
    pub drain: Vec<u8>,
}

/// Worker response.
pub struct WorkerResp<G: Signer + EcSign> {
    pub result: FetchResult,
    pub session: WireSession<G>,
}

/// A worker that replicates git objects.
struct Worker<G: Signer + EcSign> {
    storage: Storage,
    tasks: chan::Receiver<WorkerReq<G>>,
    timeout: time::Duration,
    handle: Handle<G>,
}

impl<G: Signer + EcSign + 'static> Worker<G> {
    /// Waits for tasks and runs them. Blocks indefinitely unless there is an error receiving
    /// the next task.
    fn run(mut self) -> Result<(), chan::RecvError> {
        loop {
            let task = self.tasks.recv()?;
            self.process(task);
        }
    }

    fn process(&mut self, task: WorkerReq<G>) {
        let WorkerReq {
            fetch,
            session,
            drain,
        } = task;

        let (session, result) = self._process(&fetch, drain, session);
        let result = FetchResult {
            rid: fetch.repo,
            remote: fetch.remote,
            namespaces: fetch.namespaces,
            result,
        };
        log::debug!(target: "worker", "Sending response back to service..");

        if self
            .handle
            .worker_result(WorkerResp { result, session })
            .is_err()
        {
            log::error!(target: "worker", "Unable to report fetch result: worker channel disconnected");
        }
    }

    fn _process(
        &self,
        fetch: &Fetch,
        drain: Vec<u8>,
        mut session: WireSession<G>,
    ) -> (WireSession<G>, Result<Vec<RefUpdate>, FetchError>) {
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

            if let Err(err) = session.as_connection_mut().set_nonblocking(false) {
                return (session, Err(err.into()));
            }
            let (mut stream_r, mut stream_w) = match session.split_io() {
                Ok((r, w)) => (r, w),
                Err(err) => {
                    return (err.original, Err(err.error.into()));
                }
            };
            let result = self.upload_pack(fetch, drain, &mut stream_r, &mut stream_w);
            let session = WireSession::from_split_io(stream_r, stream_w);

            (session, result)
        }
    }

    fn fetch(
        &self,
        fetch: &Fetch,
        tunnel: &mut Tunnel<WireSession<G>>,
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
            // FIXME: We need to omit our own namespace from this refspec in case we're fetching '*'.
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
            log::debug!(target: "worker", "Fetch for {} exited with status {:?}", fetch.repo, status);
        } else {
            log::debug!(target: "worker", "Fetch for {} exited with unknown status", fetch.repo);
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
        stream_r: &mut WireReader,
        stream_w: &mut WireWriter<G>,
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
        let mut reader = GitReader::new(drain, stream_r);

        match reader.read_command_pkt_line() {
            Ok(cmd) => {
                log::debug!(
                    target: "worker",
                    "Parsed git command packet-line for {}: {:?}", fetch.repo, cmd
                );
                if cmd.repo != fetch.repo {
                    return Err(FetchError::Git(git::raw::Error::from_str(
                        "git pkt-line command does not match fetch request",
                    )));
                }
            }
            Err(_) => {
                return Err(FetchError::Git(git::raw::Error::from_str(
                    "error parsing git command packet-line",
                )));
            }
        }

        thread::scope(|scope| {
            // Data coming from the remote peer is written to the standard input of the
            // `upload-pack` process.
            let t = scope.spawn(move || io::copy(&mut reader, &mut stdin));
            // Output of `upload-pack` is sent back to the remote peer.
            io::copy(&mut stdout, stream_w)?;
            // SAFETY: The thread should not panic, but if it does, we bubble up the panic.
            t.join().unwrap()?;

            Ok::<_, FetchError>(())
        })?;
        let status = child.wait()?;

        if let Some(status) = status.code() {
            log::debug!(target: "worker", "Upload-pack for {} exited with status {:?}", fetch.repo, status);
        } else {
            log::debug!(target: "worker", "Upload-pack for {} exited with unknown status", fetch.repo);
        }

        if !status.success() {
            let mut err = Vec::new();
            stderr.read_to_end(&mut err)?;

            let err = String::from_utf8_lossy(&err);
            log::debug!(target: "worker", "Upload-pack for {}: stderr: {}", fetch.repo, err);
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
    pub fn with<G: Signer + EcSign + 'static>(
        capacity: usize,
        timeout: time::Duration,
        storage: Storage,
        tasks: chan::Receiver<WorkerReq<G>>,
        handle: Handle<G>,
        name: String,
    ) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            let worker = Worker {
                tasks: tasks.clone(),
                storage: storage.clone(),
                handle: handle.clone(),
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
        for (i, worker) in self.pool.into_iter().enumerate() {
            if let Err(err) = worker.join()? {
                log::debug!(target: "pool", "Worker {i} exited: {err}");
            }
        }
        log::debug!(target: "pool", "Worker pool shutting down..");

        Ok(())
    }
}

pub struct GitReader<'a, R> {
    drain: Vec<u8>,
    stream: &'a mut R,
}

impl<'a, R: io::Read> GitReader<'a, R> {
    fn new(drain: Vec<u8>, stream: &'a mut R) -> Self {
        Self { drain, stream }
    }

    /// Parse a Git command packet-line.
    ///
    /// Example: `0032git-upload-pack /project.git\0host=myserver.com\0`
    ///
    fn read_command_pkt_line(&mut self) -> io::Result<GitCommand> {
        let mut pktline = [0u8; 1024];
        let length = self.read_pkt_line(&mut pktline)?;
        let Some(cmd) = GitCommand::parse(&pktline[..length]) else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        Ok(cmd)
    }

    /// Parse a Git packet-line.
    fn read_pkt_line(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut length = [0; 4];
        self.read_exact(&mut length)?;

        let length = str::from_utf8(&length)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        let length = usize::from_str_radix(length, 16)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        let remaining = length - 4;

        self.read_exact(&mut buf[..remaining])?;

        Ok(remaining)
    }
}

impl<'a, R: io::Read> io::Read for GitReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.drain.is_empty() {
            let count = buf.len().min(self.drain.len());
            buf[..count].copy_from_slice(&self.drain[..count]);
            self.drain.drain(..count);

            return Ok(count);
        }
        self.stream.read(buf)
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
    /// Parse a Git command from a packet-line.
    fn parse(input: &[u8]) -> Option<Self> {
        let input = str::from_utf8(input).ok()?;
        let mut parts = input
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
