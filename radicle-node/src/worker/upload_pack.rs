use std::io;
use std::io::Write;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use radicle_fetch::{ByteSlice as _, RemoteProgress};

use radicle::identity::RepoId;
use radicle::node::events;
use radicle::node::events::Emitter;
use radicle::node::{Event, NodeId};
use radicle::storage::git::paths;
use radicle::Storage;

use crate::runtime::thread;

/// Perform the Git upload-pack process, given that the Git request
/// `header` has already been read and parsed.
///
/// N.b. The upload-pack process itself is strict, i.e. it will read
/// requests from the client indefinitely, and so the client side MUST
/// send the EOF file message.
pub fn upload_pack<R, W>(
    nid: &NodeId,
    remote: NodeId,
    storage: &Storage,
    emitter: &Emitter<Event>,
    header: &pktline::GitRequest,
    mut recv: R,
    send: W,
    timeout: Duration,
) -> io::Result<ExitStatus>
where
    R: io::Read + Send,
    W: io::Write + Send,
{
    let timer = Instant::now();
    let protocol_version = header
        .extra
        .iter()
        .find_map(|kv| match kv {
            (ref k, Some(v)) if k == "version" => {
                let version = match v.as_str() {
                    "2" => 2,
                    "1" => 1,
                    _ => 0,
                };
                Some(version)
            }
            _ => None,
        })
        .unwrap_or(0);

    if protocol_version != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only Git protocol version 2 is supported",
        ));
    }

    let git_dir = paths::repository(storage, &header.repo);
    let mut child = {
        let mut cmd = Command::new("git");
        cmd.current_dir(git_dir)
            .env_clear()
            .envs(std::env::vars().filter(|(key, _)| key == "PATH" || key.starts_with("GIT_TRACE")))
            .env("GIT_PROTOCOL", format!("version={protocol_version}"))
            .args([
                "-c",
                "uploadpack.allowAnySha1InWant=true",
                "-c",
                "uploadpack.allowRefInWant=true",
                "-c",
                "lsrefs.unborn=ignore",
                "upload-pack",
                "--strict",
                format!("--timeout={}", timeout.as_secs()).as_str(),
                ".",
            ])
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::inherit());

        cmd.spawn()?
    };

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = io::BufReader::new(child.stdout.take().unwrap());
    let mut reporter = Reporter::new(header.repo, remote, emitter.clone(), send);

    thread::scope(|s| {
        thread::spawn_scoped(nid, "upload-pack", s, || {
            // N.b. we indefinitely copy stdout to the sender,
            // i.e. there's no need for a loop.
            match io::copy(&mut stdout, &mut reporter) {
                Ok(_) => {}
                Err(e) => {
                    log::error!(target: "worker", "Worker channel disconnected for {}; aborting: {e}", header.repo);
                    emitter.emit(events::UploadPack::error(header.repo, remote, e).into());
                }
            }
        });

        let reader = thread::spawn_scoped(nid, "upload-pack", s, || {
            let mut buffer = [0; u16::MAX as usize + 1];
            loop {
                match recv.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = stdin.write_all(&buffer[..n]) {
                            log::warn!(target: "worker", "Error writing to upload-pack stdin: {e}");
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        log::debug!(target: "worker", "Exiting upload-pack reader thread for {}", header.repo);
                        break;
                    }
                    // N.b. if the read timed out, ensure that the sender isn't
                    // still sending messages.
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                        log::warn!(target: "worker", "Read channel timed out for upload-pack {}", header.repo);
                        break;
                    }
                    Err(e) => {
                        log::error!(target: "worker", "Error on upload-pack channel read for {}: {e}", header.repo);
                        emitter.emit(events::UploadPack::error(header.repo, remote, e).into());
                        break;
                    }
                }
            }
        });

        // N.b. we only care if the `reader` is finished. We then kill
        // the child which will end the thread for the sender.
        if let Err(e) = reader.join() {
            log::warn!(target: "worker", "Upload pack thread panicked: {e:?}");
        }
        child.kill()?;
        Ok::<_, io::Error>(())
    })?;

    let status = child.wait()?;
    emitter.emit(events::UploadPack::done(header.repo, remote, status).into());
    log::debug!(target: "worker", "Upload pack finished ({}ms)", timer.elapsed().as_millis());
    Ok(status)
}

/// A combination of the upload-pack sender with an [`Emitter`] for reporting
/// the progress events to subscribers.
struct Reporter<W> {
    rid: RepoId,
    remote: NodeId,
    emitter: Emitter<Event>,
    send: W,
    total: usize,
}

impl<W> Reporter<W> {
    fn new(rid: RepoId, remote: NodeId, emitter: Emitter<Event>, send: W) -> Self {
        Self {
            rid,
            remote,
            emitter,
            send,
            total: 0,
        }
    }

    fn emit(&mut self, buf: &[u8]) {
        let event = match Self::as_upload_pack_progress(buf) {
            Some(progress) => events::UploadPack::write(self.rid, self.remote, progress),
            None => {
                self.total += buf.len();
                events::UploadPack::pack_progress(self.rid, self.remote, self.total)
            }
        };
        log::trace!(target: "worker", "upload-pack progress: {event:?}");
        self.emitter.emit(event.into());
    }

    fn as_upload_pack_progress(buf: &[u8]) -> Option<events::upload_pack::Progress> {
        use events::upload_pack::Progress::*;
        let RemoteProgress {
            action, step, max, ..
        } = RemoteProgress::from_bytes(buf)?;
        if action.contains_str("Counting objects") {
            step.and_then(|processed| max.map(|total| Counting { processed, total }))
        } else if action.contains_str("Compressing objects") {
            step.and_then(|processed| max.map(|total| Compressing { processed, total }))
        } else if action.contains_str("Enumerating objects") {
            max.map(|total| Enumerating { total })
        } else {
            None
        }
    }
}

impl<W> io::Write for Reporter<W>
where
    W: io::Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.send.write(buf)?;
        self.emit(buf);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send.flush()
    }
}

pub(super) mod pktline {
    use std::io;
    use std::io::Read;
    use std::str;

    use radicle::prelude::RepoId;

    pub const HEADER_LEN: usize = 4;

    /// Read and parse the `GitRequest` data from the client side.
    pub fn git_request<R>(reader: &mut R) -> io::Result<GitRequest>
    where
        R: io::Read,
    {
        let mut reader = Reader::new(reader);
        let (header, _) = reader.read_request_pktline()?;
        Ok(header)
    }

    struct Reader<'a, R> {
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
        fn read_request_pktline(&mut self) -> io::Result<(GitRequest, Vec<u8>)> {
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

    /// The Git request packet-line for a Heartwood repository.
    ///
    /// Example: `0032git-upload-pack /rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5.git\0host=myserver.com\0`
    #[derive(Debug)]
    pub struct GitRequest {
        pub repo: RepoId,
        #[allow(dead_code)]
        pub path: String,
        #[allow(dead_code)]
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
