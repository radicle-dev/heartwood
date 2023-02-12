//! Client control socket implementation.
use std::io::prelude::*;
use std::io::BufReader;
use std::io::LineWriter;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::str::FromStr;
use std::{io, net};

use radicle::node::Handle;
use serde_json as json;

use crate::identity::Id;
use crate::node::NodeId;
use crate::node::{Command, CommandName, CommandResult, FetchResult};
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
                if let Err(e) = command(&stream, &mut handle) {
                    if let CommandError::Shutdown = e {
                        log::debug!(target: "control", "Shutdown requested..");
                        // Channel might already be disconnected if shutdown
                        // came from somewhere else. Ignore errors.
                        handle.shutdown().ok();
                        break;
                    }
                    log::error!(target: "control", "Command returned error: {e}");

                    CommandResult::error(e).to_writer(&mut stream).ok();

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
    #[error("invalid command arguments `{0:?}`")]
    InvalidCommandArgs(Vec<String>),
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
    let input = line.trim_end();

    log::debug!(target: "control", "Received `{input}` on control socket");
    let cmd: Command = json::from_str(input)?;

    match cmd.name {
        CommandName::Connect => {
            todo!();
        }
        CommandName::Fetch => {
            let (rid, nid): (Id, NodeId) = parse::args(cmd)?;
            fetch(rid, nid, LineWriter::new(stream), handle)?;
        }
        CommandName::Seeds => {
            let rid: Id = parse::arg(cmd)?;
            let seeds = handle.seeds(rid)?;

            json::to_writer(writer, &seeds)?;
        }
        CommandName::TrackRepo => {
            let rid: Id = parse::arg(cmd)?;

            match handle.track_repo(rid) {
                Ok(updated) => {
                    CommandResult::Okay { updated }.to_writer(writer)?;
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            }
        }
        CommandName::UntrackRepo => {
            let rid: Id = parse::arg(cmd)?;

            match handle.untrack_repo(rid) {
                Ok(updated) => {
                    CommandResult::Okay { updated }.to_writer(writer)?;
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            }
        }
        CommandName::TrackNode => {
            let (node, alias) = match cmd.args.as_slice() {
                [node] => (node.as_str(), None),
                [node, alias] => (node.as_str(), Some(alias.to_owned())),
                _ => return Err(CommandError::InvalidCommandArgs(cmd.args)),
            };
            let nid = node
                .parse()
                .map_err(|e| CommandError::InvalidCommandArg(node.to_owned(), Box::new(e)))?;

            match handle.track_node(nid, alias) {
                Ok(updated) => {
                    CommandResult::Okay { updated }.to_writer(writer)?;
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            }
        }
        CommandName::UntrackNode => {
            let nid: NodeId = parse::arg(cmd)?;

            match handle.untrack_node(nid) {
                Ok(updated) => {
                    CommandResult::Okay { updated }.to_writer(writer)?;
                }
                Err(e) => {
                    return Err(CommandError::Runtime(e));
                }
            }
        }
        CommandName::AnnounceRefs => {
            let rid: Id = parse::arg(cmd)?;

            if let Err(e) = handle.announce_refs(rid) {
                return Err(CommandError::Runtime(e));
            }
            CommandResult::ok().to_writer(writer).ok();
        }
        CommandName::SyncInventory => match handle.sync_inventory() {
            Ok(updated) => {
                CommandResult::Okay { updated }.to_writer(writer)?;
            }
            Err(e) => {
                return Err(CommandError::Runtime(e));
            }
        },
        CommandName::Status => {
            CommandResult::ok().to_writer(writer).ok();
        }
        CommandName::Routing => match handle.routing() {
            Ok(c) => {
                for (id, seed) in c.iter() {
                    writeln!(writer, "{id} {seed}")?;
                }
            }
            Err(e) => return Err(CommandError::Runtime(e)),
        },
        CommandName::Inventory => match handle.inventory() {
            Ok(c) => {
                for id in c.iter() {
                    writeln!(writer, "{id}")?;
                }
            }
            Err(e) => return Err(CommandError::Runtime(e)),
        },
        CommandName::Shutdown => {
            return Err(CommandError::Shutdown);
        }
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

mod parse {
    use super::*;

    pub(super) fn arg<T: FromStr>(cmd: Command) -> Result<T, CommandError>
    where
        <T as FromStr>::Err: std::error::Error + 'static,
    {
        let [arg]: [String; 1] = cmd
            .args
            .clone()
            .try_into()
            .map_err(|_| CommandError::InvalidCommandArgs(cmd.args))?;

        arg.parse()
            .map_err(|e| CommandError::InvalidCommandArg(arg, Box::new(e)))
    }

    pub(super) fn args<S: FromStr, T: FromStr>(cmd: Command) -> Result<(S, T), CommandError>
    where
        <S as FromStr>::Err: std::error::Error + 'static,
        <T as FromStr>::Err: std::error::Error + 'static,
    {
        let [arg1, arg2]: [String; 2] = cmd
            .args
            .clone()
            .try_into()
            .map_err(|_| CommandError::InvalidCommandArgs(cmd.args))?;

        let arg1 = arg1
            .parse()
            .map_err(|e| CommandError::InvalidCommandArg(arg1, Box::new(e)))?;
        let arg2 = arg2
            .parse()
            .map_err(|e| CommandError::InvalidCommandArg(arg2, Box::new(e)))?;

        Ok((arg1, arg2))
    }
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
            let stream = loop {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    break stream;
                }
            };
            writeln!(
                &stream,
                "{}",
                json::to_string(&Command::new(CommandName::AnnounceRefs, [proj])).unwrap()
            )
            .unwrap();

            let stream = BufReader::new(stream);
            let line = stream.lines().next().unwrap().unwrap();

            assert_eq!(line, json::json!({ "status": "ok" }).to_string());
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
