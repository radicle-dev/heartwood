use std::io::{prelude::*, BufReader};
use std::ops::Deref;
use std::thread::JoinHandle;
use std::{env, io, net, process, thread, time};

use crossbeam_channel as chan;
use cyphernet::EcSign;
use netservices::tunnel::Tunnel;
use netservices::{NetSession, SplitIo};

use radicle::crypto::Signer;
use radicle::identity::Id;
use radicle::storage::{Namespaces, ReadRepository, RefUpdate, WriteRepository, WriteStorage};
use radicle::{git, Storage};
use reactor::poller::popol;

use crate::runtime::Handle;
use crate::service::reactor::Fetch;
use crate::storage;
use crate::wire::{WireReader, WireSession, WireWriter};

/// Worker pool configuration.
pub struct Config {
    /// Number of worker threads.
    pub capacity: usize,
    /// Whether to use atomic fetches.
    pub atomic: bool,
    /// Thread name.
    pub name: String,
    /// Timeout for all operations.
    pub timeout: time::Duration,
    /// Git daemon address.
    pub daemon: net::SocketAddr,
    /// Git storage.
    pub storage: Storage,
}

/// Result of a fetch request from a specific seed.
#[derive(Debug)]
pub struct FetchResult {
    pub fetch: Fetch,
    pub result: Result<Vec<RefUpdate>, FetchError>,
}

impl Deref for FetchResult {
    type Target = Result<Vec<RefUpdate>, FetchError>;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

/// Error returned by fetch.
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Fetch(#[from] storage::FetchError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Project(#[from] storage::ProjectError),
}

/// Task to be accomplished on a worker thread.
/// This is either going to be an outgoing or incoming fetch.
pub struct Task<G: Signer + EcSign> {
    pub fetch: Fetch,
    pub session: WireSession<G>,
    pub drain: Vec<u8>,
}

/// Worker response.
pub struct TaskResult<G: Signer + EcSign> {
    pub result: FetchResult,
    pub session: WireSession<G>,
}

/// A worker that replicates git objects.
struct Worker<G: Signer + EcSign> {
    storage: Storage,
    tasks: chan::Receiver<Task<G>>,
    daemon: net::SocketAddr,
    timeout: time::Duration,
    handle: Handle<G>,
    atomic: bool,
    name: String,
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

    fn process(&mut self, task: Task<G>) {
        let Task {
            fetch,
            session,
            drain,
        } = task;

        let (session, result) = self._process(&fetch, drain, session);
        let result = FetchResult { fetch, result };
        log::debug!(target: "worker", "Sending response back to service..");

        if self
            .handle
            .worker_result(TaskResult { result, session })
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
            log::debug!(target: "worker", "Worker processing outgoing fetch for {}", fetch.rid);

            let mut tunnel = match Tunnel::with(session, net::SocketAddr::from(([0, 0, 0, 0], 0))) {
                Ok(tunnel) => tunnel,
                Err((session, err)) => return (session, Err(err.into())),
            };
            let result = self.fetch(fetch, &mut tunnel);
            let mut session = tunnel.into_session();

            if let Err(err) = pktline::done(&mut session) {
                log::error!(target: "worker", "Fetch error: {err}");
            }
            if let Err(err) = &result {
                log::error!(target: "worker", "Fetch error: {err}");
            }
            (session, result)
        } else {
            log::debug!(target: "worker", "Worker processing incoming fetch for {}", fetch.rid);

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

            if let Err(err) = &result {
                log::error!(target: "worker", "Upload-pack error: {err}");
            }
            (session, result)
        }
    }

    fn fetch(
        &self,
        fetch: &Fetch,
        tunnel: &mut Tunnel<WireSession<G>>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let repo = self.storage.repository(fetch.rid)?;
        let tunnel_addr = tunnel.local_addr()?;
        let mut cmd = process::Command::new("git");
        cmd.current_dir(repo.path())
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("GIT_TRACE")))
            .envs(git::env::GIT_DEFAULT_CONFIG)
            .args(["-c", "protocol.version=2"])
            .arg("fetch")
            .arg("--verbose");

        match fetch.namespaces {
            Namespaces::All => {
                // We should not prune in this case, because it would mean that namespaces that
                // don't exit on the remote would be deleted locally.
            }
            Namespaces::One(_) => {
                // TODO: Make sure we verify before pruning, as pruning may get us into
                // a state we can't roll back.
                cmd.arg("--prune");
            }
        }

        if self.atomic {
            // Enable atomic fetch. Only works with Git 2.31 and later.
            cmd.arg("--atomic");
        }
        cmd.arg(format!("git://{tunnel_addr}/{}", repo.id.canonical()))
            // FIXME: We need to omit our own namespace from this refspec in case we're fetching '*'.
            .arg(fetch.namespaces.as_fetchspec())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .stdin(process::Stdio::piped());

        log::debug!(target: "worker", "Running command: {:?}", cmd);

        let mut child = cmd.spawn()?;
        let stderr = child.stderr.take().unwrap();

        thread::Builder::new().name(self.name.clone()).spawn(|| {
            for line in BufReader::new(stderr).lines().flatten() {
                log::debug!(target: "worker", "Git: {}", line);
            }
        })?;

        let _ = tunnel.tunnel_once(popol::Poller::new(), self.timeout)?;

        // TODO: Parse fetch output to return updates.
        if child.wait()?.success() {
            log::debug!(target: "worker", "Fetch for {} exited successfully", fetch.rid);
        } else {
            log::error!(target: "worker", "Fetch for {} failed", fetch.rid);
        }
        let head = repo.set_head()?;

        log::debug!(target: "worker", "Head for {} set to {head}", fetch.rid);

        Ok(vec![])
    }

    fn upload_pack(
        &self,
        fetch: &Fetch,
        drain: Vec<u8>,
        stream_r: &mut WireReader,
        stream_w: &mut WireWriter<G>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        // Connect to our local git daemon, running as a child process.
        let daemon = net::TcpStream::connect_timeout(&self.daemon, self.timeout)?;
        let (mut daemon_r, mut daemon_w) = (daemon.try_clone()?, daemon);
        let mut stream_r = pktline::Reader::new(drain, stream_r);
        let mut daemon_r = pktline::Reader::new(vec![], &mut daemon_r);
        let mut buffer = [0; u16::MAX as usize + 1];

        // Read the request packet line to make sure the repository being requested matches what
        // we expect, and that the service requested is valid.
        let request = match stream_r.read_request_pktline() {
            Ok((req, pktline)) => {
                log::debug!(
                    target: "worker",
                    "Parsed git command packet-line for {}: {:?}", fetch.rid, req
                );
                if req.repo != fetch.rid {
                    return Err(FetchError::Git(git::raw::Error::from_str(
                        "git pkt-line command does not match fetch request",
                    )));
                }
                pktline
            }
            Err(err) => {
                return Err(FetchError::Git(git::raw::Error::from_str(&format!(
                    "error parsing git command packet-line: {err}"
                ))));
            }
        };
        // Write the raw request to the daemon, once we've verified it.
        daemon_w.write_all(&request)?;

        // We now loop, alternating between reading requests from the client, and writing responses
        // back from the daemon.. Requests are delimited with a flush packet (`flush-pkt`).
        loop {
            if let Err(e) = daemon_r.read_pktlines(stream_w, &mut buffer) {
                // This is the expected error when the remote disconnects.
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                }
                log::debug!(target: "worker", "Upload of {} to {} returned error: {e}", fetch.rid, fetch.remote);

                return Err(e.into());
            }
            if let Err(e) = stream_r.read_pktlines(&mut daemon_w, &mut buffer) {
                // Triggered by a [`pktline::DONE_PKT`] packet.
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                }
                log::error!(target: "worker", "Remote returned error for {}: {e}", fetch.rid);

                return Err(e.into());
            }
        }
        log::debug!(target: "worker", "Upload of {} to {} exited successfully", fetch.rid, fetch.remote);

        // When we aren't the one fetching, no refs are updated.
        Ok(vec![])
    }
}

/// A pool of workers. One thread is allocated for each worker.
pub struct Pool {
    pool: Vec<JoinHandle<Result<(), chan::RecvError>>>,
}

impl Pool {
    /// Create a new worker pool with the given parameters.
    pub fn with<G: Signer + EcSign + 'static>(
        tasks: chan::Receiver<Task<G>>,
        handle: Handle<G>,
        config: Config,
    ) -> Self {
        let mut pool = Vec::with_capacity(config.capacity);
        for _ in 0..config.capacity {
            let worker = Worker {
                tasks: tasks.clone(),
                handle: handle.clone(),
                storage: config.storage.clone(),
                daemon: config.daemon,
                timeout: config.timeout,
                name: config.name.clone(),
                atomic: config.atomic,
            };
            let thread = thread::Builder::new()
                .name(config.name.clone())
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

mod pktline {
    use std::io;
    use std::io::Read;
    use std::str;

    use super::Id;

    pub const HEADER_LEN: usize = 4;
    pub const FLUSH_PKT: &[u8; HEADER_LEN] = b"0000";
    pub const DELIM_PKT: &[u8; HEADER_LEN] = b"0001";
    pub const RESPONSE_END_PKT: &[u8; HEADER_LEN] = b"0002";
    /// When the remote `fetch` exits, it sends a special `done` packet which triggers
    /// an EOF. This `done` packet is not part of the git protocol, and so is
    /// not sent to the deamon.
    pub const DONE_PKT: &[u8; HEADER_LEN] = b"done";

    /// Send a special `done` packet. Since the git protocol is tunneled over an existing
    /// connection, we can't signal the end of the protocol via the usual means, which is
    /// to close the connection and trigger an EOF on the other side. Git also doesn't have
    /// any special message we can send to signal the end of the protocol. Hence, we there's
    /// no other way for the server to know that we're done sending commands than to send a
    /// message that is not part of the git protocol. This message can then be processed by
    /// the remote worker to end the protocol.
    pub fn done<W: io::Write>(w: &mut W) -> io::Result<()> {
        w.write_all(DONE_PKT)
    }

    pub struct Reader<'a, R> {
        drain: Vec<u8>,
        stream: &'a mut R,
    }

    impl<'a, R: io::Read> Reader<'a, R> {
        pub fn new(drain: Vec<u8>, stream: &'a mut R) -> Self {
            Self { drain, stream }
        }

        /// Parse a Git request packet-line.
        ///
        /// Example: `0032git-upload-pack /project.git\0host=myserver.com\0`
        ///
        pub fn read_request_pktline(&mut self) -> io::Result<(GitRequest, Vec<u8>)> {
            let mut pktline = [0u8; 1024];
            let length = self.read_pktline(&mut pktline)?;
            let Some(cmd) = GitRequest::parse(&pktline[4..length]) else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            Ok((cmd, Vec::from(&pktline[..length])))
        }

        /// Parse a Git packet-line.
        pub fn read_pktline(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.read_exact(&mut buf[..HEADER_LEN])?;

            if &buf[..HEADER_LEN] == DONE_PKT {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if &buf[..HEADER_LEN] == FLUSH_PKT
                || &buf[..HEADER_LEN] == DELIM_PKT
                || &buf[..HEADER_LEN] == RESPONSE_END_PKT
            {
                return Ok(HEADER_LEN);
            }
            let length = str::from_utf8(&buf[..HEADER_LEN])
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
            let length = usize::from_str_radix(length, 16)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;

            self.read_exact(&mut buf[HEADER_LEN..length])?;

            Ok(length)
        }

        pub fn read_pktlines<W: io::Write>(&mut self, w: &mut W, buf: &mut [u8]) -> io::Result<()> {
            loop {
                let n = self.read_pktline(buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;

                if &buf[..n] == FLUSH_PKT {
                    return Ok(());
                }
            }
            Ok(())
        }
    }

    impl<'a, R: io::Read> io::Read for Reader<'a, R> {
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
    pub struct GitRequest {
        pub repo: Id,
        pub path: String,
        pub host: Option<(String, Option<u16>)>,
        pub extra: Vec<(String, Option<String>)>,
    }

    impl GitRequest {
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
}
