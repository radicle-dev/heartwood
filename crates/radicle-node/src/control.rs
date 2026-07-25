//! Client control socket implementation.
use std::io::BufReader;
use std::io::LineWriter;
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{io, net, time};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

use radicle::identity::RepoId;
use radicle::node::Handle;
use radicle::node::NodeId;
use radicle::node::{Command, CommandResult};
use radicle::storage::refs;
use serde_json as json;

use crate::runtime;
use crate::runtime::thread;

/// Maximum timeout for waiting for node events.
const MAX_TIMEOUT: time::Duration = time::Duration::MAX;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to bind control socket listener: {0}")]
    Bind(io::Error),
    #[error("invalid socket path specified: {0}")]
    InvalidPath(PathBuf),
    #[error("node: {0}")]
    Node(#[from] runtime::handle::Error),
}

/// Listen until the runtime requests shutdown. The listener must be non-blocking.
pub(crate) fn listen<E, H>(
    listener: UnixListener,
    handle: H,
    stopping: Arc<AtomicBool>,
) -> Result<(), Error>
where
    H: Handle<Error = runtime::handle::Error> + 'static,
    H::Events: EventStream<Event = H::Event>,
    H::Sessions: serde::Serialize,
    CommandResult<E>: From<H::Event>,
    E: serde::Serialize,
{
    log::debug!(target: "control", "Control thread listening on socket..");
    let nid = handle.nid()?;
    let mut handlers = Vec::new();
    while !stopping.load(Ordering::Acquire) {
        reap_finished(&mut handlers);
        match listener.accept() {
            Ok((mut stream, _)) => {
                let handle = handle.clone();
                let stopping = stopping.clone();
                let shutdown = stream.try_clone().ok();
                let join = thread::spawn(&nid, "control", move || {
                    if let Err(e) = command(&stream, handle, stopping) {
                        log::debug!(target: "control", "Command returned error: {e}");
                        CommandResult::error(e).to_writer(&mut stream).ok();
                    }
                    stream.flush().ok();
                    stream.shutdown(net::Shutdown::Both).ok();
                });
                handlers.push((shutdown, join));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(time::Duration::from_millis(25));
            }
            Err(e) => log::warn!(target: "control", "Failed to accept control connection: {e}"),
        }
    }
    log::debug!(target: "control", "Shutting down.");
    for (stream, _) in &handlers {
        if let Some(stream) = stream {
            let _ = stream.shutdown(net::Shutdown::Both);
        }
    }
    for (_, handler) in handlers {
        let _ = handler.join();
    }
    Ok(())
}

/// Drop socket clones and join handlers which have already completed.
///
/// Socket clones are retained only so shutdown can interrupt handlers blocked
/// in a command. Keeping them after the handler exits would otherwise prevent
/// clients from observing EOF and leak one descriptor per command.
fn reap_finished(handlers: &mut Vec<(Option<UnixStream>, std::thread::JoinHandle<()>)>) {
    let mut index = 0;
    while index < handlers.len() {
        if handlers[index].1.is_finished() {
            let (_, handler) = handlers.swap_remove(index);
            let _ = handler.join();
        } else {
            index += 1;
        }
    }
}

#[derive(thiserror::Error, Debug)]
enum CommandError {
    #[error("(de)serialization failed: {0}")]
    Serialization(#[from] json::Error),
    #[error("runtime error: {0}")]
    Runtime(#[from] runtime::handle::Error),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

pub(crate) enum EventPoll<T> {
    Event(T),
    Timeout,
    Disconnected,
}

pub(crate) trait EventStream {
    type Event;

    fn recv_timeout(&mut self, timeout: time::Duration) -> EventPoll<Self::Event>;
}

impl EventStream for radicle::node::Events {
    type Event = radicle::node::Event;

    fn recv_timeout(&mut self, timeout: time::Duration) -> EventPoll<Self::Event> {
        match std::sync::mpsc::Receiver::recv_timeout(self, timeout) {
            Ok(event) => EventPoll::Event(event),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => EventPoll::Timeout,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => EventPoll::Disconnected,
        }
    }
}

#[cfg(test)]
impl<T> EventStream for Vec<T> {
    type Event = T;

    fn recv_timeout(&mut self, _timeout: time::Duration) -> EventPoll<Self::Event> {
        if self.is_empty() {
            EventPoll::Disconnected
        } else {
            EventPoll::Event(self.remove(0))
        }
    }
}

fn command<E, H>(
    stream: &UnixStream,
    mut handle: H,
    stopping: Arc<AtomicBool>,
) -> Result<(), CommandError>
where
    H: Handle<Error = runtime::handle::Error> + 'static,
    H::Events: EventStream<Event = H::Event>,
    H::Sessions: serde::Serialize,
    CommandResult<E>: From<H::Event>,
    E: serde::Serialize,
{
    let mut reader = BufReader::new(stream);
    let mut writer = LineWriter::new(stream);

    let mut line = String::new();

    reader.read_line(&mut line)?;
    let input = line.trim_end();

    log::debug!(target: "control", "Received `{input}` on control socket");
    let cmd: Command = json::from_str(input)?;

    match cmd {
        Command::Connect { addr, opts } => {
            let (nid, addr) = addr.into_pair();
            match handle.connect(nid, addr, opts) {
                Err(e) => return Err(CommandError::Runtime(e)),
                Ok(result) => {
                    json::to_writer(&mut writer, &result)?;
                    writer.write_all(b"\n")?;
                }
            }
        }
        Command::Disconnect { nid } => match handle.disconnect(nid) {
            Err(e) => return Err(CommandError::Runtime(e)),
            Ok(()) => {
                CommandResult::ok().to_writer(writer).ok();
            }
        },
        Command::Fetch {
            rid,
            nid,
            timeout,
            signed_references_minimum_feature_level,
        } => {
            fetch(
                rid,
                nid,
                timeout,
                signed_references_minimum_feature_level,
                writer,
                &mut handle,
            )?;
        }
        Command::Config => {
            let config = handle.config()?;

            CommandResult::Okay(config).to_writer(writer)?;
        }
        Command::ListenAddrs => {
            let addrs = handle.listen_addrs()?;

            CommandResult::Okay(addrs).to_writer(writer)?;
        }
        Command::SeedsFor { rid, namespaces } => {
            let seeds = handle.seeds_for(rid, namespaces)?;

            CommandResult::Okay(seeds).to_writer(writer)?;
        }
        Command::Sessions => {
            let sessions = handle.sessions()?;

            CommandResult::Okay(sessions).to_writer(writer)?;
        }
        Command::Session { nid } => {
            let session = handle.session(nid)?;

            CommandResult::Okay(session).to_writer(writer)?;
        }
        Command::Seed { rid, scope } => match handle.seed(rid, scope) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Unseed { rid } => match handle.unseed(rid) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Follow { nid, alias } => match handle.follow(nid, alias) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Block { nid } => match handle.block(nid) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Unfollow { nid } => match handle.unfollow(nid) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::AnnounceRefsFor { rid, namespaces } => {
            let refs = handle.announce_refs_for(rid, namespaces)?;

            CommandResult::Okay(refs).to_writer(writer)?;
        }
        Command::AnnounceInventory => {
            if let Err(e) = handle.announce_inventory() {
                return Err(CommandError::Runtime(e));
            }
            CommandResult::ok().to_writer(writer).ok();
        }
        Command::AddInventory { rid } => match handle.add_inventory(rid) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Subscribe => match handle.subscribe(MAX_TIMEOUT) {
            Ok(mut events) => {
                while !stopping.load(Ordering::Acquire) {
                    match events.recv_timeout(time::Duration::from_millis(100)) {
                        EventPoll::Event(event) => {
                            CommandResult::from(event).to_writer(&mut writer)?
                        }
                        EventPoll::Timeout => {}
                        EventPoll::Disconnected => break,
                    }
                }
            }
            Err(e) => return Err(CommandError::Runtime(e)),
        },
        Command::Status => {
            CommandResult::ok().to_writer(writer).ok();
        }
        Command::NodeId => match handle.nid() {
            Ok(nid) => {
                CommandResult::Okay(nid).to_writer(writer)?;
            }
            Err(e) => return Err(CommandError::Runtime(e)),
        },
        Command::Debug => {
            let debug = handle.debug()?;

            CommandResult::Okay(debug).to_writer(writer)?;
        }
        Command::Shutdown => {
            log::debug!(target: "control", "Shutdown requested..");
            // Channel might already be disconnected if shutdown
            // came from somewhere else. Ignore errors.
            handle.shutdown().ok();
            CommandResult::ok().to_writer(writer).ok();
        }
    }
    Ok(())
}

fn fetch<W: Write, H: Handle<Error = runtime::handle::Error>>(
    id: RepoId,
    node: NodeId,
    timeout: time::Duration,
    signed_references_minimum_feature_level: Option<refs::FeatureLevel>,
    mut writer: W,
    handle: &mut H,
) -> Result<(), CommandError> {
    match handle.fetch(id, node, timeout, signed_references_minimum_feature_level) {
        Ok(result) => {
            json::to_writer(&mut writer, &result)?;
            writer.write_all(b"\n")?;
        }
        Err(e) => {
            return Err(CommandError::Runtime(e));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::thread;

    use radicle::identity::RepoId;
    use radicle::node::Handle;
    use radicle::node::policy::Scope;
    use radicle::node::{Alias, FetchResult, Node, NodeId};
    use radicle::test::arbitrary;

    use crate::test;

    #[test]
    fn test_control_socket() {
        let tmp = tempfile::tempdir().unwrap();
        let handle = test::handle::Handle::default();
        let socket = tmp.path().join("alice.sock");
        let rids = arbitrary::set::<RepoId>(1..3);
        let listener = UnixListener::bind(&socket).unwrap();
        let nid = handle.nid().unwrap();
        let stopping = Arc::new(AtomicBool::new(false));

        thread::spawn({
            let handle = handle.clone();

            move || listen(listener, handle, stopping)
        });

        for rid in &rids {
            let mut stream = loop {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    break stream;
                }
            };
            writeln!(
                &mut stream,
                "{}",
                json::to_string(&Command::AnnounceRefsFor {
                    rid: rid.to_owned(),
                    namespaces: [nid].into(),
                })
                .unwrap()
            )
            .unwrap();

            let stream = BufReader::new(stream);
            let line = stream.lines().next().unwrap().unwrap();

            assert_eq!(
                line,
                json::json!({
                    "remote": handle.nid().unwrap(),
                    "at": "0000000000000000000000000000000000000000"
                })
                .to_string()
            );
        }

        for rid in &rids {
            assert!(handle.updates.lock().unwrap().contains(&(*rid, nid)));
        }

        // Fetch results are serialized directly rather than through
        // `CommandResult`, so make sure they are still newline-terminated.
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(time::Duration::from_secs(1)))
            .unwrap();
        Command::Fetch {
            rid: *rids.iter().next().unwrap(),
            nid: arbitrary::r#gen::<NodeId>(1),
            timeout: time::Duration::from_secs(1),
            signed_references_minimum_feature_level: None,
        }
        .to_writer(&mut stream)
        .unwrap();

        let line = BufReader::new(stream).lines().next().unwrap().unwrap();
        let result: FetchResult = json::from_str(&line).unwrap();
        assert!(result.is_success());
    }

    #[test]
    fn test_seed_unseed() {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("node.sock");
        let proj = arbitrary::r#gen::<RepoId>(1);
        let peer = arbitrary::r#gen::<NodeId>(1);
        let listener = UnixListener::bind(&socket).unwrap();
        let mut handle = Node::new(&socket);
        let stopping = Arc::new(AtomicBool::new(false));

        thread::spawn({
            let handle = crate::test::handle::Handle::default();

            move || crate::control::listen(listener, handle, stopping)
        });

        // Wait for node to be online.
        while !handle.is_running() {}

        assert!(handle.seed(proj, Scope::default()).unwrap());
        assert!(!handle.seed(proj, Scope::default()).unwrap());
        assert!(handle.unseed(proj).unwrap());
        assert!(!handle.unseed(proj).unwrap());

        assert!(handle.follow(peer, Some(Alias::new("alice"))).unwrap());
        assert!(!handle.follow(peer, Some(Alias::new("alice"))).unwrap());
        assert!(handle.unfollow(peer).unwrap());
        assert!(!handle.unfollow(peer).unwrap());
    }
}
