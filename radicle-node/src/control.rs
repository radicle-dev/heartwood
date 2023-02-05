//! Client control socket implementation.
use std::io::prelude::*;
use std::io::BufReader;
use std::io::LineWriter;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::{io, net};

use radicle::node::Handle;
use serde_json as json;

use crate::identity::Id;
use crate::node;
use crate::node::FetchResult;
use crate::node::NodeId;
use crate::runtime;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to bind control socket listener: {0}")]
    Bind(io::Error),
    #[error("invalid socket path specified: {0}")]
    InvalidPath(PathBuf),
}

/// Listen for commands on the control socket, and process them.
pub fn listen<H: Handle<Error = runtime::HandleError, FetchResult = FetchResult>>(
    listener: UnixListener,
    mut handle: H,
) -> Result<(), Error> {
    log::debug!(target: "control", "Control thread listening on socket..");

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                log::debug!(target: "control", "Accepted new client on control socket..");

                if let Err(e) = command(&stream, &mut handle) {
                    if let CommandError::Shutdown = e {
                        log::debug!(target: "control", "Shutdown requested..");
                        // Channel might already be disconnected if shutdown
                        // came from somewhere else. Ignore errors.
                        handle.shutdown().ok();
                        break;
                    }
                    writeln!(stream, "error: {e}").ok();

                    stream.flush().ok();
                    stream.shutdown(net::Shutdown::Both).ok();
                }
            }
            Err(e) => log::error!(target: "control", "Failed to accept incoming connection: {}", e),
        }
    }
    log::debug!(target: "control", "Exiting control loop..");

    Ok(())
}

#[derive(thiserror::Error, Debug)]
enum CommandError {
    #[error("invalid command argument `{0}`, {1}")]
    InvalidCommandArg(String, Box<dyn std::error::Error>),
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("serialization failed: {0}")]
    Serialization(#[from] json::Error),
    #[error("runtime error: {0}")]
    Runtime(#[from] runtime::HandleError),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("shutdown requested")]
    Shutdown,
}

fn command<H: Handle<Error = runtime::HandleError, FetchResult = FetchResult>>(
    stream: &UnixStream,
    handle: &mut H,
) -> Result<(), CommandError> {
    let mut reader = BufReader::new(stream);
    let mut writer = LineWriter::new(stream);
    let mut line = String::new();

    reader.read_line(&mut line)?;

    let cmd = line.trim_end();

    log::debug!(target: "control", "Received `{cmd}` on control socket");

    // TODO: refactor to include helper
    match cmd.split_once(' ') {
        Some(("fetch", args)) => {
            if let Some((rid, node)) = args.split_once(' ') {
                let rid: Id = rid
                    .parse()
                    .map_err(|e| CommandError::InvalidCommandArg(rid.to_owned(), Box::new(e)))?;
                let node: NodeId = node
                    .parse()
                    .map_err(|e| CommandError::InvalidCommandArg(node.to_owned(), Box::new(e)))?;

                fetch(rid, node, LineWriter::new(stream), handle)?;
            }
        }
        Some(("seeds", arg)) => {
            let rid: Id = arg
                .parse()
                .map_err(|e| CommandError::InvalidCommandArg(arg.to_owned(), Box::new(e)))?;

            for seed in handle.seeds(rid)? {
                writeln!(writer, "{seed}")?;
            }
        }
        Some(("track-repo", arg)) => match arg.parse() {
            Ok(id) => match handle.track_repo(id) {
                Ok(updated) => {
                    if updated {
                        writeln!(writer, "{}", node::RESPONSE_OK)?;
                    } else {
                        writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                    }
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            },
            Err(err) => {
                return Err(CommandError::InvalidCommandArg(
                    arg.to_owned(),
                    Box::new(err),
                ));
            }
        },
        Some(("untrack-repo", arg)) => match arg.parse() {
            Ok(id) => match handle.untrack_repo(id) {
                Ok(updated) => {
                    if updated {
                        writeln!(writer, "{}", node::RESPONSE_OK)?;
                    } else {
                        writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                    }
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            },
            Err(err) => {
                return Err(CommandError::InvalidCommandArg(
                    arg.to_owned(),
                    Box::new(err),
                ));
            }
        },
        Some(("track-node", args)) => {
            let (peer, alias) = if let Some((peer, alias)) = args.split_once(' ') {
                (peer, Some(alias.to_owned()))
            } else {
                (args, None)
            };
            match peer.parse() {
                Ok(id) => match handle.track_node(id, alias) {
                    Ok(updated) => {
                        if updated {
                            writeln!(writer, "{}", node::RESPONSE_OK)?;
                        } else {
                            writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                        }
                    }
                    Err(e) => {
                        return Err(CommandError::Runtime(e));
                    }
                },
                Err(err) => {
                    return Err(CommandError::InvalidCommandArg(
                        args.to_owned(),
                        Box::new(err),
                    ));
                }
            }
        }
        Some(("untrack-node", arg)) => match arg.parse() {
            Ok(id) => match handle.untrack_node(id) {
                Ok(updated) => {
                    if updated {
                        writeln!(writer, "{}", node::RESPONSE_OK)?;
                    } else {
                        writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                    }
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            },
            Err(err) => {
                return Err(CommandError::InvalidCommandArg(
                    arg.to_owned(),
                    Box::new(err),
                ));
            }
        },
        Some(("announce-refs", arg)) => match arg.parse() {
            Ok(id) => {
                if let Err(e) = handle.announce_refs(id) {
                    return Err(CommandError::Runtime(e));
                }
                writeln!(writer, "{}", node::RESPONSE_OK)?;
            }
            Err(err) => {
                return Err(CommandError::InvalidCommandArg(
                    arg.to_owned(),
                    Box::new(err),
                ));
            }
        },
        Some((cmd, _)) => return Err(CommandError::UnknownCommand(cmd.to_owned())),

        // Commands with no arguments.
        None => match cmd {
            "status" => {
                writeln!(writer, "{}", node::RESPONSE_OK).ok();
            }
            "routing" => match handle.routing() {
                Ok(c) => {
                    for (id, seed) in c.iter() {
                        writeln!(writer, "{id} {seed}",)?;
                    }
                }
                Err(e) => return Err(CommandError::Runtime(e)),
            },
            "inventory" => match handle.inventory() {
                Ok(c) => {
                    for id in c.iter() {
                        writeln!(writer, "{id}")?;
                    }
                }
                Err(e) => return Err(CommandError::Runtime(e)),
            },
            "shutdown" => {
                return Err(CommandError::Shutdown);
            }
            _ => {
                return Err(CommandError::UnknownCommand(line));
            }
        },
    }
    Ok(())
}

fn fetch<W: Write, H: Handle<Error = runtime::HandleError, FetchResult = FetchResult>>(
    id: Id,
    node: NodeId,
    mut writer: W,
    handle: &mut H,
) -> Result<(), CommandError> {
    match handle.fetch(id, node) {
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
    use crate::node::{Node, NodeId};
    use crate::test;

    #[test]
    fn test_control_socket() {
        let tmp = tempfile::tempdir().unwrap();
        let handle = test::handle::Handle::default();
        let socket = tmp.path().join("alice.sock");
        let projs = test::arbitrary::set::<Id>(1..3);
        let listener = UnixListener::bind(&socket).unwrap();

        thread::spawn({
            let handle = handle.clone();

            move || listen(listener, handle)
        });

        for proj in &projs {
            let mut buf = [0; 2];
            let mut stream = loop {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    break stream;
                }
            };
            writeln!(&stream, "announce-refs {proj}").unwrap();
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, &[b'o', b'k']);
        }

        for proj in &projs {
            assert!(handle.updates.lock().unwrap().contains(proj));
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

        assert!(handle.track_repo(proj).unwrap());
        assert!(!handle.track_repo(proj).unwrap());
        assert!(handle.untrack_repo(proj).unwrap());
        assert!(!handle.untrack_repo(proj).unwrap());

        assert!(handle
            .track_node(peer, Some(String::from("alice")))
            .unwrap());
        assert!(!handle
            .track_node(peer, Some(String::from("alice")))
            .unwrap());
        assert!(handle.untrack_node(peer).unwrap());
        assert!(!handle.untrack_node(peer).unwrap());
    }
}
