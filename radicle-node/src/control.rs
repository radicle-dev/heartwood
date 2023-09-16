//! Client control socket implementation.
use std::io::prelude::*;
use std::io::BufReader;
use std::io::LineWriter;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::{io, net, time};

use radicle::node::Handle;
use serde_json as json;

use crate::identity::Id;
use crate::node::NodeId;
use crate::node::{Command, CommandResult};
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
    Node(#[from] runtime::HandleError),
}

/// Listen for commands on the control socket, and process them.
pub fn listen<H: Handle<Error = runtime::HandleError> + 'static>(
    listener: UnixListener,
    handle: H,
) -> Result<(), Error>
where
    H::Sessions: serde::Serialize,
{
    log::debug!(target: "control", "Control thread listening on socket..");
    let nid = handle.nid()?;

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let handle = handle.clone();

                thread::spawn(&nid, "control", move || {
                    if let Err(e) = command(&stream, handle) {
                        log::error!(target: "control", "Command returned error: {e}");

                        CommandResult::error(e).to_writer(&mut stream).ok();

                        stream.flush().ok();
                        stream.shutdown(net::Shutdown::Both).ok();
                    }
                });
            }
            Err(e) => log::error!(target: "control", "Failed to accept incoming connection: {}", e),
        }
    }
    log::debug!(target: "control", "Exiting control loop..");

    Ok(())
}

#[derive(thiserror::Error, Debug)]
enum CommandError {
    #[error("(de)serialization failed: {0}")]
    Serialization(#[from] json::Error),
    #[error("runtime error: {0}")]
    Runtime(#[from] runtime::HandleError),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

fn command<H: Handle<Error = runtime::HandleError> + 'static>(
    stream: &UnixStream,
    mut handle: H,
) -> Result<(), CommandError>
where
    H::Sessions: serde::Serialize,
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
            let (nid, addr) = addr.into();
            match handle.connect(nid, addr, opts) {
                Err(e) => return Err(CommandError::Runtime(e)),
                Ok(result) => {
                    json::to_writer(&mut writer, &result)?;
                    writer.write_all(b"\n")?;
                }
            }
        }
        Command::Fetch { rid, nid, timeout } => {
            fetch(rid, nid, timeout, writer, &mut handle)?;
        }
        Command::Config => {
            let config = handle.config()?;

            json::to_writer(writer, &config)?;
        }
        Command::Seeds { rid } => {
            let seeds = handle.seeds(rid)?;

            json::to_writer(writer, &seeds)?;
        }
        Command::Sessions => {
            let sessions = handle.sessions()?;

            json::to_writer(writer, &sessions)?;
        }
        Command::TrackRepo { rid, scope } => match handle.track_repo(rid, scope) {
            Ok(updated) => {
                CommandResult::Okay { updated }.to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::UntrackRepo { rid } => match handle.untrack_repo(rid) {
            Ok(updated) => {
                CommandResult::Okay { updated }.to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::TrackNode { nid, alias } => match handle.track_node(nid, alias) {
            Ok(updated) => {
                CommandResult::Okay { updated }.to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::UntrackNode { nid } => match handle.untrack_node(nid) {
            Ok(updated) => {
                CommandResult::Okay { updated }.to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::AnnounceRefs { rid } => {
            if let Err(e) = handle.announce_refs(rid) {
                return Err(CommandError::Runtime(e));
            }
            CommandResult::ok().to_writer(writer).ok();
        }
        Command::AnnounceInventory => {
            if let Err(e) = handle.announce_inventory() {
                return Err(CommandError::Runtime(e));
            }
            CommandResult::ok().to_writer(writer).ok();
        }
        Command::SyncInventory => match handle.sync_inventory() {
            Ok(updated) => {
                CommandResult::Okay { updated }.to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Subscribe => match handle.subscribe(MAX_TIMEOUT) {
            Ok(events) => {
                for e in events {
                    let event = e?;
                    let event = serde_json::to_string(&event)?;

                    writeln!(&mut writer, "{event}")?;
                }
            }
            Err(e) => log::error!(target: "control", "Error subscribing to events: {e}"),
        },
        Command::Status => {
            CommandResult::ok().to_writer(writer).ok();
        }
        Command::NodeId => match handle.nid() {
            Ok(nid) => {
                writeln!(writer, "{nid}")?;
            }
            Err(e) => return Err(CommandError::Runtime(e)),
        },
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

fn fetch<W: Write, H: Handle<Error = runtime::HandleError>>(
    id: Id,
    node: NodeId,
    timeout: time::Duration,
    mut writer: W,
    handle: &mut H,
) -> Result<(), CommandError> {
    match handle.fetch(id, node, timeout) {
        Ok(result) => {
            json::to_writer(&mut writer, &result)?;
        }
        Err(e) => {
            return Err(CommandError::Runtime(e));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::prelude::*;
    use std::os::unix::net::UnixStream;
    use std::thread;

    use super::*;
    use crate::identity::Id;
    use crate::node::Handle;
    use crate::node::{Alias, Node, NodeId};
    use crate::service::tracking::Scope;
    use crate::test;

    #[test]
    fn test_control_socket() {
        let tmp = tempfile::tempdir().unwrap();
        let handle = test::handle::Handle::default();
        let socket = tmp.path().join("alice.sock");
        let rids = test::arbitrary::set::<Id>(1..3);
        let listener = UnixListener::bind(&socket).unwrap();

        thread::spawn({
            let handle = handle.clone();

            move || listen(listener, handle)
        });

        for rid in &rids {
            let stream = loop {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    break stream;
                }
            };
            writeln!(
                &stream,
                "{}",
                json::to_string(&Command::AnnounceRefs {
                    rid: rid.to_owned()
                })
                .unwrap()
            )
            .unwrap();

            let stream = BufReader::new(stream);
            let line = stream.lines().next().unwrap().unwrap();

            assert_eq!(line, json::json!({ "status": "ok" }).to_string());
        }

        for rid in &rids {
            assert!(handle.updates.lock().unwrap().contains(rid));
        }
    }

    #[test]
    fn test_track_untrack() {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("node.sock");
        let proj = test::arbitrary::gen::<Id>(1);
        let peer = test::arbitrary::gen::<NodeId>(1);
        let listener = UnixListener::bind(&socket).unwrap();
        let mut handle = Node::new(&socket);

        thread::spawn({
            let handle = crate::test::handle::Handle::default();

            move || crate::control::listen(listener, handle)
        });

        // Wait for node to be online.
        while !handle.is_running() {}

        assert!(handle.track_repo(proj, Scope::default()).unwrap());
        assert!(!handle.track_repo(proj, Scope::default()).unwrap());
        assert!(handle.untrack_repo(proj).unwrap());
        assert!(!handle.untrack_repo(proj).unwrap());

        assert!(handle.track_node(peer, Some(Alias::new("alice"))).unwrap());
        assert!(!handle.track_node(peer, Some(Alias::new("alice"))).unwrap());
        assert!(handle.untrack_node(peer).unwrap());
        assert!(!handle.untrack_node(peer).unwrap());
    }
}
