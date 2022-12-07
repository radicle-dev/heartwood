//! Client control socket implementation.
use std::io::prelude::*;
use std::io::BufReader;
use std::io::LineWriter;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::{fs, io, net};

use crate::client;
use crate::client::handle::traits::Handle;
use crate::identity::Id;
use crate::node;
use crate::service::FetchLookup;
use crate::service::FetchResult;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to bind control socket listener: {0}")]
    Bind(io::Error),
    #[error("invalid socket path specified: {0}")]
    InvalidPath(PathBuf),
}

/// Listen for commands on the control socket, and process them.
pub fn listen<P: AsRef<Path>, H: Handle>(path: P, mut handle: H) -> Result<(), Error> {
    // Remove the socket file on startup before rebinding.
    fs::remove_file(&path).ok();
    fs::create_dir_all(
        path.as_ref()
            .parent()
            .ok_or_else(|| Error::InvalidPath(path.as_ref().to_path_buf()))?,
    )
    .ok();

    log::info!("Binding control socket {}..", path.as_ref().display());

    let listener = UnixListener::bind(path).map_err(Error::Bind)?;
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(e) = drain(&stream, &mut handle) {
                    log::error!("Received {} on control socket", e);

                    writeln!(stream, "error: {}", e).ok();

                    stream.flush().ok();
                    stream.shutdown(net::Shutdown::Both).ok();
                } else {
                    writeln!(stream, "ok").ok();
                }
            }
            Err(e) => log::error!("Failed to open control socket stream: {}", e),
        }
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
enum DrainError {
    #[error("invalid command argument `{0}`")]
    InvalidCommandArg(String),
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("client error: {0}")]
    Client(#[from] client::handle::Error),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

fn drain<H: Handle>(stream: &UnixStream, handle: &mut H) -> Result<(), DrainError> {
    let mut reader = BufReader::new(stream);
    let mut writer = LineWriter::new(stream);

    // TODO: refactor to include helper
    for line in reader.by_ref().lines().flatten() {
        match line.split_once(' ') {
            Some(("fetch", arg)) => {
                if let Ok(id) = arg.parse() {
                    fetch(id, LineWriter::new(stream), handle)?;
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some(("track-repo", arg)) => {
                if let Ok(id) = arg.parse() {
                    match handle.track_repo(id) {
                        Ok(updated) => {
                            if updated {
                                writeln!(writer, "{}", node::RESPONSE_OK)?;
                            } else {
                                writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                            }
                        }
                        Err(e) => {
                            return Err(DrainError::Client(e));
                        }
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some(("untrack-repo", arg)) => {
                if let Ok(id) = arg.parse() {
                    match handle.untrack_repo(id) {
                        Ok(updated) => {
                            if updated {
                                writeln!(writer, "{}", node::RESPONSE_OK)?;
                            } else {
                                writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                            }
                        }
                        Err(e) => {
                            return Err(DrainError::Client(e));
                        }
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some(("track-node", args)) => {
                let (peer, alias) = if let Some((peer, alias)) = args.split_once(' ') {
                    (peer, Some(alias.to_owned()))
                } else {
                    (args, None)
                };
                if let Ok(id) = peer.parse() {
                    match handle.track_node(id, alias) {
                        Ok(updated) => {
                            if updated {
                                writeln!(writer, "{}", node::RESPONSE_OK)?;
                            } else {
                                writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                            }
                        }
                        Err(e) => {
                            return Err(DrainError::Client(e));
                        }
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(args.to_owned()));
                }
            }
            Some(("untrack-node", arg)) => {
                if let Ok(id) = arg.parse() {
                    match handle.untrack_node(id) {
                        Ok(updated) => {
                            if updated {
                                writeln!(writer, "{}", node::RESPONSE_OK)?;
                            } else {
                                writeln!(writer, "{}", node::RESPONSE_NOOP)?;
                            }
                        }
                        Err(e) => {
                            return Err(DrainError::Client(e));
                        }
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some(("announce-refs", arg)) => {
                if let Ok(id) = arg.parse() {
                    if let Err(e) = handle.announce_refs(id) {
                        return Err(DrainError::Client(e));
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some((cmd, _)) => return Err(DrainError::UnknownCommand(cmd.to_owned())),

            // Commands with no arguments.
            None => match line.as_str() {
                "routing" => match handle.routing() {
                    Ok(c) => {
                        for (id, seed) in c.iter() {
                            writeln!(writer, "{id} {seed}",)?;
                        }
                    }
                    Err(e) => return Err(DrainError::Client(e)),
                },
                "inventory" => match handle.inventory() {
                    Ok(c) => {
                        for id in c.iter() {
                            writeln!(writer, "{id}")?;
                        }
                    }
                    Err(e) => return Err(DrainError::Client(e)),
                },
                _ => {
                    return Err(DrainError::UnknownCommand(line));
                }
            },
        }
    }
    Ok(())
}

fn fetch<W: Write, H: Handle>(id: Id, mut writer: W, handle: &mut H) -> Result<(), DrainError> {
    match handle.fetch(id) {
        Err(e) => {
            return Err(DrainError::Client(e));
        }
        Ok(FetchLookup::Found { seeds, results }) => {
            let seeds = Vec::from(seeds);

            writeln!(
                writer,
                "ok: found {} seeds for {} ({:?})",
                seeds.len(),
                &id,
                &seeds,
            )?;

            for result in results.iter() {
                match result {
                    FetchResult::Fetched { from, updated } => {
                        writeln!(writer, "ok: {} fetched from {}", &id, from)?;

                        for update in updated {
                            writeln!(writer, "{}", update)?;
                        }
                    }
                    FetchResult::Error { from, error } => {
                        writeln!(
                            writer,
                            "error: {} failed to fetch from {}: {}",
                            &id, from, error
                        )?;
                    }
                }
            }
        }
        Ok(FetchLookup::NotFound) => {
            writeln!(writer, "error: {} was not found", &id)?;
        }
        Ok(FetchLookup::NotTracking) => {
            writeln!(writer, "error: {} is not tracked", &id)?;
        }
        Ok(FetchLookup::Error(err)) => {
            writeln!(writer, "error: {}", err)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::prelude::*;
    use std::os::unix::net::UnixStream;
    use std::{net, thread};

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

        thread::spawn({
            let socket = socket.clone();
            let handle = handle.clone();

            move || listen(socket, handle)
        });

        let mut stream = loop {
            if let Ok(stream) = UnixStream::connect(&socket) {
                break stream;
            }
        };
        for proj in &projs {
            writeln!(&stream, "announce-refs {}", proj).unwrap();
        }

        let mut buf = [0; 2];
        stream.shutdown(net::Shutdown::Write).unwrap();
        stream.read_exact(&mut buf).unwrap();

        assert_eq!(&buf, &[b'o', b'k']);
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

        thread::spawn({
            let socket = socket.clone();
            let handle = crate::test::handle::Handle::default();

            move || crate::control::listen(socket, handle)
        });

        let handle = loop {
            if let Ok(conn) = Node::connect(&socket) {
                break conn;
            }
        };

        assert!(handle.track_repo(&proj).unwrap());
        assert!(!handle.track_repo(&proj).unwrap());
        assert!(handle.untrack_repo(&proj).unwrap());
        assert!(!handle.untrack_repo(&proj).unwrap());

        assert!(handle.track_node(&peer, Some("alice")).unwrap());
        assert!(!handle.track_node(&peer, Some("alice")).unwrap());
        assert!(handle.untrack_node(&peer).unwrap());
        assert!(!handle.untrack_node(&peer).unwrap());
    }
}
