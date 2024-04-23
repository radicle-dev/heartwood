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

use crate::identity::RepoId;
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
        Command::Disconnect { nid } => match handle.disconnect(nid) {
            Err(e) => return Err(CommandError::Runtime(e)),
            Ok(()) => {
                CommandResult::ok().to_writer(writer).ok();
            }
        },
        Command::Fetch { rid, nid, timeout } => {
            fetch(rid, nid, timeout, writer, &mut handle)?;
        }
        Command::Config => {
            let config = handle.config()?;

            CommandResult::Okay(config).to_writer(writer)?;
        }
        Command::ListenAddrs => {
            let addrs = handle.listen_addrs()?;

            CommandResult::Okay(addrs).to_writer(writer)?;
        }
        Command::Seeds { rid } => {
            let seeds = handle.seeds(rid)?;

            CommandResult::Okay(seeds).to_writer(writer)?;
        }
        Command::Sessions => {
            let sessions = handle.sessions()?;

            CommandResult::Okay(sessions).to_writer(writer)?;
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
        Command::Unfollow { nid } => match handle.unfollow(nid) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::AnnounceRefs { rid } => {
            let refs = handle.announce_refs(rid)?;

            CommandResult::Okay(refs).to_writer(writer)?;
        }
        Command::AnnounceInventory => {
            if let Err(e) = handle.announce_inventory() {
                return Err(CommandError::Runtime(e));
            }
            CommandResult::ok().to_writer(writer).ok();
        }
        Command::UpdateInventory { rid } => match handle.update_inventory(rid) {
            Ok(result) => {
                CommandResult::updated(result).to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        Command::Subscribe => match handle.subscribe(MAX_TIMEOUT) {
            Ok(events) => {
                for e in events {
                    let event = e?;
                    CommandResult::Okay(event).to_writer(&mut writer)?;
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
    id: RepoId,
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
    use crate::identity::RepoId;
    use crate::node::Handle;
    use crate::node::{Alias, Node, NodeId};
    use crate::service::policy::Scope;
    use crate::test;

    #[test]
    fn test_control_socket() {
        let tmp = tempfile::tempdir().unwrap();
        let handle = test::handle::Handle::default();
        let socket = tmp.path().join("alice.sock");
        let rids = test::arbitrary::set::<RepoId>(1..3);
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
            assert!(handle.updates.lock().unwrap().contains(rid));
        }
    }

    #[test]
    fn test_seed_unseed() {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("node.sock");
        let proj = test::arbitrary::gen::<RepoId>(1);
        let peer = test::arbitrary::gen::<NodeId>(1);
        let listener = UnixListener::bind(&socket).unwrap();
        let mut handle = Node::new(&socket);

        thread::spawn({
            let handle = crate::test::handle::Handle::default();

            move || crate::control::listen(listener, handle)
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
