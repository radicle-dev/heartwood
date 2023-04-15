mod channels;
mod fetch;
mod tunnel;

use std::io::{prelude::*, BufReader};
use std::ops::ControlFlow;
use std::thread::JoinHandle;
use std::{env, io, net, process, thread, time};

use crossbeam_channel as chan;

use radicle::identity::{Id, IdentityError};
use radicle::prelude::NodeId;
use radicle::storage::{Namespaces, ReadRepository, RefUpdate};
use radicle::{git, Storage};

use crate::runtime::Handle;
use crate::storage;
use crate::wire::StreamId;
use channels::{ChannelReader, ChannelWriter};
use tunnel::Tunnel;

pub use channels::{ChannelEvent, Channels};

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
    #[error("worker channel error: {0}")]
    Channel(#[from] chan::SendError<ChannelEvent>),
    #[error(transparent)]
    StagingInit(#[from] fetch::error::Init),
    #[error(transparent)]
    StagingTransition(#[from] fetch::error::Transition),
    #[error(transparent)]
    StagingTransfer(#[from] fetch::error::Transfer),
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

/// Fetch job sent to worker thread.
#[derive(Debug, Clone)]
pub enum Fetch {
    /// Client is initiating a fetch in order to receive the specified
    /// `refspecs` determined by [`Namespaces`].
    Initiator {
        /// Repo to fetch.
        rid: Id,
        /// Namespaces to fetch.
        namespaces: Namespaces,
        /// Remote peer we are interacting with.
        remote: NodeId,
    },
    /// Server is responding to a fetch request by uploading the
    /// specified `refspecs` sent by the client.
    Responder {
        /// Remote peer we are interacting with.
        remote: NodeId,
    },
}

impl Fetch {
    pub fn remote(&self) -> NodeId {
        match self {
            Self::Initiator { remote, .. } | Self::Responder { remote } => *remote,
        }
    }

    pub fn initiated(self) -> Option<(Id, Namespaces)> {
        match self {
            Self::Initiator {
                rid, namespaces, ..
            } => Some((rid, namespaces)),
            Self::Responder { .. } => None,
        }
    }
}

/// Task to be accomplished on a worker thread.
/// This is either going to be an outgoing or incoming fetch.
pub struct Task {
    pub fetch: Fetch,
    pub stream: StreamId,
    pub channels: Channels,
}

/// Worker response.
#[derive(Debug)]
pub struct TaskResult {
    pub fetch: Fetch,
    pub stream: StreamId,
    pub result: Result<Vec<RefUpdate>, FetchError>,
}

/// A worker that replicates git objects.
struct Worker {
    nid: NodeId,
    storage: Storage,
    tasks: chan::Receiver<Task>,
    daemon: net::SocketAddr,
    timeout: time::Duration,
    handle: Handle,
    atomic: bool,
    name: String,
}

impl Worker {
    /// Waits for tasks and runs them. Blocks indefinitely unless there is an error receiving
    /// the next task.
    fn run(mut self) -> Result<(), chan::RecvError> {
        loop {
            let task = self.tasks.recv()?;
            self.process(task);
        }
    }

    fn process(&mut self, task: Task) {
        let Task {
            fetch,
            channels,
            stream,
        } = task;
        let result = self._process(&fetch, stream, channels);

        log::trace!(target: "worker", "Sending response back to service..");

        if self
            .handle
            .worker_result(TaskResult {
                fetch,
                stream,
                result,
            })
            .is_err()
        {
            log::error!(target: "worker", "Unable to report fetch result: worker channel disconnected");
        }
    }

    fn _process(
        &mut self,
        fetch: &Fetch,
        stream: StreamId,
        mut channels: Channels,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        match &fetch {
            Fetch::Initiator {
                rid,
                namespaces,
                remote,
            } => {
                log::debug!(target: "worker", "Worker processing outgoing fetch for {}", rid);

                self.fetch(*rid, *remote, stream, namespaces, channels)
            }
            Fetch::Responder { .. } => {
                log::debug!(target: "worker", "Worker processing incoming fetch..");

                let (stream_w, stream_r) = channels.split();
                // Nb. two fetches are usually expected: one for the *special* refs,
                // followed by another for the signed refs.
                loop {
                    match self.upload_pack(fetch, stream, stream_r, stream_w) {
                        Ok(ControlFlow::Continue(())) => continue,
                        Ok(ControlFlow::Break(())) => break,
                        Err(e) => return Err(e.into()),
                    }
                }
                log::debug!(target: "worker", "Upload process on stream {stream} exited successfully");

                Ok(vec![])
            }
        }
    }

    fn fetch(
        &mut self,
        rid: Id,
        remote: NodeId,
        stream: StreamId,
        namespaces: &Namespaces,
        mut channels: Channels,
    ) -> Result<Vec<RefUpdate>, FetchError> {
        let staging = fetch::StagingPhaseInitial::new(&self.storage, rid, namespaces.clone())?;
        match self._fetch(
            &staging.repo,
            remote,
            staging.refspecs(),
            stream,
            &mut channels,
        ) {
            Ok(()) => log::debug!(target: "worker", "Initial fetch for {rid} exited successfully"),
            Err(e) => match (&staging.repo, e) {
                // When fetching, if the error comes from `git-fetch` returning an error, we
                // keep going because it could be due to a rejected ref (eg. on `rad/sigrefs`),
                // and that is non-fatal.
                (fetch::StagedRepository::Fetching(_), e @ FetchError::CommandFailed { .. }) => {
                    log::warn!(target: "worker", "Initial fetch for {rid} returned an error: {e}");
                    log::warn!(target: "worker", "It's possible that some of the refs were rejected..");
                }
                // When cloning, any error is fatal, since we'll end up with an empty repository.
                // Likewise, when fetching, if the error is coming from some other place, we
                // abort the fetch.
                (fetch::StagedRepository::Cloning(_) | fetch::StagedRepository::Fetching(_), e) => {
                    log::error!(target: "worker", "Initial fetch for {rid} failed: {e}");
                    return Err(e);
                }
            },
        }

        let staging = staging.into_final()?;
        match self._fetch(
            &staging.repo,
            remote,
            staging.refspecs(),
            stream,
            &mut channels,
        ) {
            Ok(()) => log::debug!(target: "worker", "Final fetch for {rid} exited successfully"),
            Err(e) => {
                log::error!(target: "worker", "Final fetch for {rid} failed: {e}");
                return Err(e);
            }
        }

        staging.transfer().map_err(FetchError::from)
    }

    fn upload_pack(
        &mut self,
        fetch: &Fetch,
        stream: StreamId,
        stream_r: &mut ChannelReader,
        stream_w: &mut ChannelWriter,
    ) -> Result<ControlFlow<()>, UploadError> {
        log::debug!(target: "worker", "Waiting for Git request pktline from {}..", fetch.remote());

        // Read the request packet line to make sure the repository being requested matches what
        // we expect, and that the service requested is valid.
        let (rid, request) = match pktline::Reader::new(stream_r).read_request_pktline() {
            Ok((req, pktline)) => (req.repo, pktline),
            Err(err) if err.kind() == io::ErrorKind::ConnectionReset => {
                log::debug!(
                    target: "worker",
                    "Upload process received stream `close` from {}", fetch.remote()
                );
                return Ok(ControlFlow::Break(()));
            }
            Err(err) => {
                return Err(UploadError::InvalidPacketLine(err));
            }
        };
        log::debug!(target: "worker", "Received Git request pktline for {rid}..");

        match self._upload_pack(rid, fetch.remote(), request, stream, stream_r, stream_w) {
            Ok(()) => {
                log::debug!(target: "worker", "Upload of {rid} to {} exited successfully", fetch.remote());

                Ok(ControlFlow::Continue(()))
            }
            Err(e) => Err(e),
        }
    }

    fn _upload_pack(
        &mut self,
        rid: Id,
        remote: NodeId,
        request: Vec<u8>,
        stream: StreamId,
        stream_r: &mut ChannelReader,
        stream_w: &mut ChannelWriter,
    ) -> Result<(), UploadError> {
        log::debug!(target: "worker", "Connecting to daemon..");

        // Connect to our local git daemon, running as a child process.
        let daemon = net::TcpStream::connect_timeout(&self.daemon, self.timeout)
            .map_err(UploadError::DaemonConnectionFailed)?;
        let (mut daemon_r, mut daemon_w) = (daemon.try_clone()?, daemon);

        // Write the raw request to the daemon, once we've parsed it.
        daemon_w.write_all(&request)?;

        log::debug!(target: "worker", "Entering Git protocol loop for {rid}..");

        thread::scope(|s| {
            let daemon_to_stream = thread::Builder::new().name(self.name.clone()).spawn_scoped(s, || {
                let mut buffer = [0; u16::MAX as usize + 1];

                loop {
                    match daemon_r.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            stream_w.send(buffer[..n].to_vec())?;

                            if let Err(e) = self.handle.flush(remote, stream) {
                                log::error!(target: "worker", "Worker channel disconnected; aborting");
                                return Err(e);
                            }
                        }
                        Err(e) => {
                            if e.kind() == io::ErrorKind::UnexpectedEof {
                                log::debug!(target: "worker", "Daemon closed the git connection for {rid}");
                                break;
                            }
                            return Err(e);
                        }
                    }
                }
                Self::eof(remote, stream, stream_w, &mut self.handle)
            })?;

            let stream_to_daemon = s.spawn(move || {
                stream_r
                    .pipe(&mut daemon_w)
                    .and_then(|()| daemon_w.shutdown(net::Shutdown::Both))
            });

            stream_to_daemon.join().unwrap()?;
            daemon_to_stream.join().unwrap()?;

            Ok::<(), UploadError>(())
        })
    }

    fn _fetch<S>(
        &self,
        repo: &fetch::StagedRepository,
        remote: NodeId,
        specs: S,
        stream: StreamId,
        channels: &mut Channels,
    ) -> Result<(), FetchError>
    where
        S: fetch::AsRefspecs,
    {
        let mut tunnel = Tunnel::with(channels, stream, self.nid, remote, self.handle.clone())?;
        let tunnel_addr = tunnel.local_addr();
        let mut cmd = process::Command::new("git");
        cmd.current_dir(repo.path())
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("GIT_TRACE")))
            .envs(git::env::GIT_DEFAULT_CONFIG)
            .args(["-c", "protocol.version=2"])
            .arg("fetch")
            .arg("--verbose");

        if self.atomic {
            // Enable atomic fetch. Only works with Git 2.31 and later.
            cmd.arg("--atomic");
        }

        let namespace = self.nid.to_namespace();
        let mut fetchspecs = specs
            .into_refspecs()
            .into_iter()
            // Filter out our own refs, if we aren't cloning.
            .filter(|fs| repo.is_cloning() || !fs.dst.starts_with(namespace.as_str()))
            .map(|spec| spec.to_string())
            .collect::<Vec<_>>();

        if !repo.is_cloning() {
            // Make sure we don't fetch our own refs via a glob pattern.
            fetchspecs.push(format!("^refs/namespaces/{}/*", self.nid));
        }

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

        tunnel.run(self.timeout)?;

        let result = child.wait()?;
        if result.success() {
            Ok(())
        } else {
            Err(FetchError::CommandFailed {
                code: result.code().unwrap_or(1),
            })
        }
    }

    fn eof(
        remote: NodeId,
        stream: StreamId,
        sender: &mut ChannelWriter,
        handle: &mut Handle,
    ) -> Result<(), io::Error> {
        log::debug!(target: "worker", "Sending end-of-file to remote {remote}..");

        if sender.eof().is_err() {
            log::error!(target: "worker", "Fetch error: error sending end-of-file message: channel disconnected");
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if let Err(e) = handle.flush(remote, stream) {
            log::error!(target: "worker", "Error flushing worker stream: {e}");
        }
        Ok(())
    }
}

/// A pool of workers. One thread is allocated for each worker.
pub struct Pool {
    pool: Vec<JoinHandle<Result<(), chan::RecvError>>>,
}

impl Pool {
    /// Create a new worker pool with the given parameters.
    pub fn with(nid: NodeId, tasks: chan::Receiver<Task>, handle: Handle, config: Config) -> Self {
        let mut pool = Vec::with_capacity(config.capacity);
        for _ in 0..config.capacity {
            let worker = Worker {
                nid,
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
    use std::str;

    use super::Id;

    pub const HEADER_LEN: usize = 4;

    pub struct Reader<'a, R> {
        stream: &'a mut R,
    }

    impl<'a, R: io::Read> Reader<'a, R> {
        /// Create a new packet-line reader.
        pub fn new(stream: &'a mut R) -> Self {
            Self { stream }
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
        fn read_pktline(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.read_exact(&mut buf[..HEADER_LEN])?;

            let length = str::from_utf8(&buf[..HEADER_LEN])
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
            let length = usize::from_str_radix(length, 16)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;

            self.read_exact(&mut buf[HEADER_LEN..length])?;

            Ok(length)
        }
    }

    impl<'a, R: io::Read> io::Read for Reader<'a, R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
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
