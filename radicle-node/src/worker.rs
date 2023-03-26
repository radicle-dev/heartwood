use std::io::{prelude::*, BufReader};
use std::ops::ControlFlow;
use std::thread::JoinHandle;
use std::{env, io, net, process, thread, time};

use crossbeam_channel as chan;
use cyphernet::Ecdh;
use netservices::tunnel::Tunnel;
use netservices::{AsConnection, NetSession, SplitIo};

use radicle::crypto::{PublicKey, Signer};
use radicle::identity::{Id, IdentityError};
use radicle::storage::{Namespaces, ReadRepository, RefUpdate, WriteRepository, WriteStorage};
use radicle::{git, Storage};
use reactor::poller::popol;

use crate::runtime::Handle;
use crate::service::reactor::{Fetch, FetchDirection};
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

/// Error returned by fetch.
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("the 'git fetch' command failed with exit code '{code}'")]
    CommandFailed { code: i32 },
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Fetch(#[from] storage::FetchError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("upload failed: {0}")]
    Upload(#[from] UploadError),
    #[error("remote aborted fetch")]
    RemoteAbortedFetch,
}

impl FetchError {
    /// Check if it's a timeout error.
    pub fn is_timeout(&self) -> bool {
        matches!(self, FetchError::Io(e) if e.kind() == io::ErrorKind::TimedOut)
    }
}

/// Error returned by fetch responder.
#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("worker failed to connect to git daemon: {0}")]
    DaemonConnectionFailed(io::Error),
    #[error("git pkt-line command does not match fetch request")]
    CommandMismatch,
    #[error("error parsing git command packet-line: {0}")]
    InvalidPacketLine(io::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl UploadError {
    /// Check if it's an end-of-file error.
    pub fn is_eof(&self) -> bool {
        matches!(self, UploadError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof)
    }
}

/// Task to be accomplished on a worker thread.
/// This is either going to be an outgoing or incoming fetch.
pub struct Task<G: Signer + Ecdh> {
    pub fetch: Fetch,
    pub session: WireSession<G>,
    pub drain: Vec<u8>,
}

/// Worker response.
pub struct TaskResult<G: Signer + Ecdh> {
    pub fetch: Fetch,
    pub result: Result<Vec<RefUpdate>, FetchError>,
    pub session: WireSession<G>,
}

/// A worker that replicates git objects.
struct Worker<G: Signer + Ecdh> {
    local: PublicKey,
    storage: Storage,
    tasks: chan::Receiver<Task<G>>,
    daemon: net::SocketAddr,
    timeout: time::Duration,
    handle: Handle<G>,
    atomic: bool,
    name: String,
}

impl<G: Signer + Ecdh + 'static> Worker<G> {
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

        let timeout = session.as_connection().read_timeout().unwrap_or_default();
        let (session, result) = self._process(&fetch, drain, session);
        // In case the timeout is changed during the fetch, we reset it here.
        session.as_connection().set_read_timeout(timeout).ok();

        log::debug!(target: "worker", "Sending response back to service..");

        if self
            .handle
            .worker_result(TaskResult {
                fetch,
                result,
                session,
            })
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
        let rid = fetch.rid;
        match &fetch.direction {
            FetchDirection::Initiator { namespaces } => {
                log::debug!(target: "worker", "Worker processing outgoing fetch for {}", fetch.rid);

                let mut tunnel =
                    match Tunnel::with(session, net::SocketAddr::from(([0, 0, 0, 0], 0))) {
                        Ok(tunnel) => tunnel,
                        Err((session, err)) => return (session, Err(err.into())),
                    };
                let result = self.fetch(rid, namespaces, &mut tunnel);
                let mut session = tunnel.into_session();

                if let Err(err) = &result {
                    log::error!(target: "worker", "Fetch error: {err}");
                }
                log::debug!(target: "worker", "Sending `done` packet to remote..");

                if let Err(err) = pktline::done(&mut session) {
                    log::error!(target: "worker", "Fetch error: error sending `done` packet: {err}");
                }
                (session, result)
            }
            FetchDirection::Responder => {
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
                let mut pktline_r = pktline::Reader::new(drain, &mut stream_r);

                match self.upload_pack(fetch, &mut pktline_r, &mut stream_w) {
                    Ok(()) => {
                        log::debug!(target: "worker", "Upload of {} to {} exited successfully", fetch.rid, fetch.remote);

                        (WireSession::from_split_io(stream_r, stream_w), Ok(vec![]))
                    }
                    Err(err) => {
                        log::error!(target: "worker", "Upload error for {rid}: {err}");

                        // If we exited without receiving a `done` packet, wait for it here.
                        // It's possible that the daemon exited first, or the remote crashed.
                        log::debug!(target: "worker", "Waiting for `done` packet from remote..");
                        let mut header = [0; pktline::HEADER_LEN];

                        // Set the read timeout for the `done` packet to twice the configured
                        // value that is used for the fetching (initiator) side.
                        //
                        // This is because the uploader always waits for the `done` packet;
                        // so in case the fetch is aborted by the uploader, eg. if
                        // it can't connect with the daemon, it will wait long enough for the
                        // fetcher to timeout before timing out itself, and will thus receive
                        // the `done` packet.
                        pktline_r
                            .stream()
                            .as_connection()
                            .set_read_timeout(Some(self.timeout * 2))
                            .ok();

                        loop {
                            match pktline_r.read_done_pktline(&mut header) {
                                Ok(()) => {
                                    log::debug!(target: "worker", "Received `done` packet from remote");

                                    // If we get the `done` packet, we exit with the original
                                    // error.
                                    return (
                                        WireSession::from_split_io(stream_r, stream_w),
                                        Err(err.into()),
                                    );
                                }
                                Err(e) if e.kind() == io::ErrorKind::InvalidInput => {
                                    // If we get some other packet, because the fetch request
                                    // is still sending stuff, we simply keep reading until we
                                    // get a `done` packet.
                                    continue;
                                }
                                Err(_) => {
                                    // If we get any other error, eg. a timeout, we abort.
                                    log::error!(
                                        target: "worker",
                                        "Upload of {} to {} aborted: missing `done` packet from remote",
                                        fetch.rid,
                                        fetch.remote
                                    );
                                    return (
                                        WireSession::from_split_io(stream_r, stream_w),
                                        Err(FetchError::RemoteAbortedFetch),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn fetch(
        &self,
        rid: Id,
        namespaces: &Namespaces,
        tunnel: &mut Tunnel<WireSession<G>>,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let repo = match self.storage.repository_mut(rid) {
            Ok(r) => Ok(r),
            Err(e) if e.is_not_found() => self.storage.create(rid),
            Err(e) => Err(e),
        }?;
        let tunnel_addr = tunnel.local_addr()?;
        let mut cmd = process::Command::new("git");
        cmd.current_dir(repo.path())
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("GIT_TRACE")))
            .envs(git::env::GIT_DEFAULT_CONFIG)
            .args(["-c", "protocol.version=2"])
            .arg("fetch")
            .arg("--verbose");

        match namespaces {
            Namespaces::All => {
                // We should not prune in this case, because it would mean that namespaces that
                // don't exit on the remote would be deleted locally.
            }
            Namespaces::One(_) => {
                // TODO: Make sure we verify before pruning, as pruning may get us into
                // a state we can't roll back.
                cmd.arg("--prune");
            }
            Namespaces::Many(_) => {
                // Same case as All
            }
        }

        if self.atomic {
            // Enable atomic fetch. Only works with Git 2.31 and later.
            cmd.arg("--atomic");
        }

        // Ignore our own remote when fetching
        let mut fetchspecs = namespaces.as_fetchspecs();
        fetchspecs.push(format!("^refs/namespaces/{}/*", self.local));

        cmd.arg(format!("git://{tunnel_addr}/{}", repo.id.canonical()))
            .args(&fetchspecs)
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
        let result = child.wait()?;
        if result.success() {
            log::debug!(target: "worker", "Fetch for {} exited successfully", rid);
            let head = repo.set_head()?;
            log::debug!(target: "worker", "Head for {} set to {head}", rid);
            let head = repo.set_identity_head()?;
            log::debug!(target: "worker", "'refs/rad/id' for {} set to {head}", rid);
            Ok(vec![])
        } else {
            log::error!(target: "worker", "Fetch for {} failed", rid);
            Err(FetchError::CommandFailed {
                code: result.code().unwrap_or(1),
            })
        }
    }

    fn upload_pack(
        &self,
        fetch: &Fetch,
        stream_r: &mut pktline::Reader<WireReader>,
        stream_w: &mut WireWriter<G>,
    ) -> Result<(), UploadError> {
        // Read the request packet line to make sure the repository being requested matches what
        // we expect, and that the service requested is valid.
        let request = match stream_r.read_request_pktline() {
            Ok((req, pktline)) => {
                log::debug!(
                    target: "worker",
                    "Parsed git command packet-line for {}: {:?}", fetch.rid, req
                );
                if req.repo != fetch.rid {
                    return Err(UploadError::CommandMismatch);
                }
                pktline
            }
            Err(err) => {
                return Err(UploadError::InvalidPacketLine(err));
            }
        };

        // Connect to our local git daemon, running as a child process.
        let daemon = net::TcpStream::connect_timeout(&self.daemon, self.timeout)
            .map_err(UploadError::DaemonConnectionFailed)?;
        let (mut daemon_r, mut daemon_w) = (daemon.try_clone()?, daemon);
        let mut daemon_r = pktline::Reader::new(vec![], &mut daemon_r);

        // Write the raw request to the daemon, once we've verified it.
        daemon_w.write_all(&request)?;

        // We now loop, alternating between reading requests from the client, and writing responses
        // back from the daemon.. Requests are delimited with a flush packet (`flush-pkt`).
        let mut buffer = [0; u16::MAX as usize + 1];
        loop {
            // Read from the daemon and write to the stream.
            if let Err(e) = daemon_r.pipe(stream_w, &mut buffer) {
                // This is the expected error when the daemon disconnects.
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    log::debug!(target: "worker", "Daemon closed the git connection for {}", fetch.rid);
                }
                return Err(e.into());
            }
            // Read from the stream and write to the daemon.
            match stream_r.pipe(&mut daemon_w, &mut buffer) {
                // Triggered by a [`pktline::DONE_PKT`] packet.
                Ok(ControlFlow::Break(())) => {
                    log::debug!(target: "worker", "Received `done` packet from remote for {}", fetch.rid);
                    return Ok(());
                }
                Ok(ControlFlow::Continue(())) => {
                    continue;
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        log::debug!(target: "worker", "Remote closed the git connection for {}", fetch.rid);
                    }
                    return Err(e.into());
                }
            }
        }
    }
}

/// A pool of workers. One thread is allocated for each worker.
pub struct Pool {
    pool: Vec<JoinHandle<Result<(), chan::RecvError>>>,
}

impl Pool {
    /// Create a new worker pool with the given parameters.
    pub fn with<G: Signer + Ecdh + 'static>(
        local: PublicKey,
        tasks: chan::Receiver<Task<G>>,
        handle: Handle<G>,
        config: Config,
    ) -> Self {
        let mut pool = Vec::with_capacity(config.capacity);
        for _ in 0..config.capacity {
            let worker = Worker {
                local,
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
                log::trace!(target: "pool", "Worker {i} exited: {err}");
            }
        }
        log::debug!(target: "pool", "Worker pool shutting down..");

        Ok(())
    }
}

pub mod pktline {
    use std::io;
    use std::io::Read;
    use std::ops::ControlFlow;
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

    /// Packetline read result.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Packetline {
        /// Received a `done` control packet.
        Done,
        /// Received a git packet with the given length.
        Git(usize),
    }

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
        /// Create a new packet-line reader.
        pub fn new(drain: Vec<u8>, stream: &'a mut R) -> Self {
            Self { drain, stream }
        }

        /// Return the underlying stream.
        pub fn stream(&self) -> &R {
            self.stream
        }

        /// Parse a Git request packet-line.
        ///
        /// Example: `0032git-upload-pack /project.git\0host=myserver.com\0`
        ///
        pub fn read_request_pktline(&mut self) -> io::Result<(GitRequest, Vec<u8>)> {
            let mut pktline = [0u8; 1024];
            let Packetline::Git(length) = self.read_pktline(&mut pktline)? else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            let Some(cmd) = GitRequest::parse(&pktline[4..length]) else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            Ok((cmd, Vec::from(&pktline[..length])))
        }

        /// Parse a `done` packet-line.
        pub fn read_done_pktline(&mut self, buf: &mut [u8]) -> io::Result<()> {
            self.read_exact(&mut buf[..HEADER_LEN])?;

            if &buf[..HEADER_LEN] == DONE_PKT {
                return Ok(());
            }
            Err(io::ErrorKind::InvalidInput.into())
        }

        /// Parse a Git packet-line.
        pub fn read_pktline(&mut self, buf: &mut [u8]) -> io::Result<Packetline> {
            self.read_exact(&mut buf[..HEADER_LEN])?;

            if &buf[..HEADER_LEN] == DONE_PKT {
                return Ok(Packetline::Done);
            }
            if &buf[..HEADER_LEN] == FLUSH_PKT
                || &buf[..HEADER_LEN] == DELIM_PKT
                || &buf[..HEADER_LEN] == RESPONSE_END_PKT
            {
                return Ok(Packetline::Git(HEADER_LEN));
            }
            let length = str::from_utf8(&buf[..HEADER_LEN])
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
            let length = usize::from_str_radix(length, 16)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;

            self.read_exact(&mut buf[HEADER_LEN..length])?;

            Ok(Packetline::Git(length))
        }

        /// Read packet-lines from the internal reader into `buf`,
        /// and write them to the given writer.
        ///
        /// Returns [`ControlFlow::Break`] if the fetch should be terminated.
        /// Otherwise, returns [`ControlFlow::Continue`] to mean that we're
        /// expecting a response from the remote.
        pub fn pipe<W: io::Write>(
            &mut self,
            w: &mut W,
            buf: &mut [u8],
        ) -> io::Result<ControlFlow<()>> {
            loop {
                let n = match self.read_pktline(buf)? {
                    Packetline::Done => return Ok(ControlFlow::Break(())),
                    Packetline::Git(n) => n,
                };
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;

                if &buf[..n] == FLUSH_PKT {
                    break;
                }
            }
            Ok(ControlFlow::Continue(()))
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
