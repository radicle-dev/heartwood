//! Client control socket implementation.
use std::io::prelude::*;
use std::io::BufReader;
use std::io::LineWriter;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::{io, net};

use radicle::node::Handle;

use crate::identity::Id;
use crate::node;
use crate::node::FetchLookup;
use crate::runtime;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to bind control socket listener: {0}")]
    Bind(io::Error),
    #[error("invalid socket path specified: {0}")]
    InvalidPath(PathBuf),
}

/// Listen for commands on the control socket, and process them.
pub fn listen<H: Handle<Error = runtime::HandleError>>(
    listener: UnixListener,
    mut handle: H,
) -> Result<(), Error> {
    log::debug!(target: "control", "Control thread listening on socket..");

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                log::debug!(target: "control", "Accepted new client on control socket..");

                if let Err(e) = drain(&stream, &mut handle) {
                    if let DrainError::Shutdown = e {
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
enum DrainError {
    #[error("invalid command argument `{0}`, {1}")]
    InvalidCommandArg(String, Box<dyn std::error::Error>),
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("runtime error: {0}")]
    Runtime(#[from] runtime::HandleError),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("shutdown requested")]
    Shutdown,
}

fn drain<H: Handle<Error = runtime::HandleError>>(
    stream: &UnixStream,
    handle: &mut H,
) -> Result<(), DrainError> {
    let mut reader = BufReader::new(stream);
    let mut writer = LineWriter::new(stream);
    let mut line = String::new();

    reader.read_line(&mut line)?;

    let cmd = line.trim_end();

    log::debug!(target: "control", "Received `{cmd}` on control socket");

    // TODO: refactor to include helper
    match cmd.split_once(' ') {
        Some(("fetch", arg)) => match arg.parse() {
            Ok(id) => {
                fetch(id, LineWriter::new(stream), handle)?;
            }
            Err(err) => {
                return Err(DrainError::InvalidCommandArg(arg.to_owned(), Box::new(err)));
            }
        },
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
                    return Err(DrainError::Runtime(e));
                }
            },
            Err(err) => {
                return Err(DrainError::InvalidCommandArg(arg.to_owned(), Box::new(err)));
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
                    return Err(DrainError::Runtime(e));
                }
            },
            Err(err) => {
                return Err(DrainError::InvalidCommandArg(arg.to_owned(), Box::new(err)));
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
                        return Err(DrainError::Runtime(e));
                    }
                },
                Err(err) => {
                    return Err(DrainError::InvalidCommandArg(
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
                    return Err(DrainError::Runtime(e));
                }
            },
            Err(err) => {
                return Err(DrainError::InvalidCommandArg(arg.to_owned(), Box::new(err)));
            }
        },
        Some(("announce-refs", arg)) => match arg.parse() {
            Ok(id) => {
                if let Err(e) = handle.announce_refs(id) {
                    return Err(DrainError::Runtime(e));
                }
                writeln!(writer, "{}", node::RESPONSE_OK)?;
            }
            Err(err) => {
                return Err(DrainError::InvalidCommandArg(arg.to_owned(), Box::new(err)));
            }
        },
        Some((cmd, _)) => return Err(DrainError::UnknownCommand(cmd.to_owned())),

        // Commands with no arguments.
        None => match cmd {
            "status" => {
                println!("RECEIVED 'status'");
                writeln!(writer, "{}", node::RESPONSE_OK).ok();
            }
            "routing" => match handle.routing() {
                Ok(c) => {
                    for (id, seed) in c.iter() {
                        writeln!(writer, "{id} {seed}",)?;
                    }
                }
                Err(e) => return Err(DrainError::Runtime(e)),
            },
            "inventory" => match handle.inventory() {
                Ok(c) => {
                    for id in c.iter() {
                        writeln!(writer, "{id}")?;
                    }
                }
                Err(e) => return Err(DrainError::Runtime(e)),
            },
            "shutdown" => {
                return Err(DrainError::Shutdown);
            }
            _ => {
                return Err(DrainError::UnknownCommand(line));
            }
        },
    }
    Ok(())
}

fn fetch<W: Write, H: Handle<Error = runtime::HandleError>>(
    id: Id,
    mut writer: W,
    handle: &mut H,
) -> Result<(), DrainError> {
    match handle.fetch(id) {
        Err(e) => {
            return Err(DrainError::Runtime(e));
        }
        Ok(FetchLookup::Found { seeds, results }) => {
            let seeds = Vec::from(seeds);

            writeln!(
                writer,
                "ok: found {} seeds for {} ({:?})", // TODO: Better output
                seeds.len(),
                &id,
                &seeds,
            )?;

            for result in results
                .iter()
                .take(results.capacity().unwrap_or(seeds.len()))
            {
                match result.result {
                    Ok(updated) => {
                        writeln!(writer, "ok: {id} fetched from {}", result.remote)?;
                        for update in updated {
                            writeln!(writer, "{update}")?;
                        }
                    }
                    Err(err) => {
                        writeln!(
                            writer,
                            "error: {id} failed to fetch from {}: {err}",
                            result.remote
                        )?;
                    }
                }
            }
        }
        Ok(FetchLookup::NotFound) => {
            writeln!(writer, "error: {id} was not found")?;
        }
        Ok(FetchLookup::NotTracking) => {
            writeln!(writer, "error: {id} is not tracked")?;
        }
        Ok(FetchLookup::Error(err)) => {
            writeln!(writer, "error: {err}")?;
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
