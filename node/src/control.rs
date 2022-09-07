//! Client control socket implementation.
use std::io::prelude::*;
use std::io::BufReader;
use std::io::LineWriter;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::{fs, io, net};

use crate::client;
use crate::client::handle::traits::Handle;
use crate::identity::ProjId;
use crate::protocol::FetchLookup;
use crate::protocol::FetchResult;

/// Default name for control socket file.
pub const DEFAULT_SOCKET_NAME: &str = "radicle.sock";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to bind control socket listener: {0}")]
    Bind(io::Error),
}

/// Listen for commands on the control socket, and process them.
pub fn listen<P: AsRef<Path>, H: Handle>(path: P, handle: H) -> Result<(), Error> {
    // Remove the socket file on startup before rebinding.
    fs::remove_file(&path).ok();

    let listener = UnixListener::bind(path).map_err(Error::Bind)?;
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(e) = drain(&stream, &handle) {
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
    #[error("invalid command")]
    InvalidCommand,
    #[error("client error: {0}")]
    Client(#[from] client::handle::Error),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

fn drain<H: Handle>(stream: &UnixStream, handle: &H) -> Result<(), DrainError> {
    let mut reader = BufReader::new(stream);

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
            Some(("track", arg)) => {
                if let Ok(id) = arg.parse() {
                    if let Err(e) = handle.track(id) {
                        return Err(DrainError::Client(e));
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some(("untrack", arg)) => {
                if let Ok(id) = arg.parse() {
                    if let Err(e) = handle.untrack(id) {
                        return Err(DrainError::Client(e));
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some(("update", arg)) => {
                if let Ok(id) = arg.parse() {
                    if let Err(e) = handle.updated(id) {
                        return Err(DrainError::Client(e));
                    }
                } else {
                    return Err(DrainError::InvalidCommandArg(arg.to_owned()));
                }
            }
            Some((cmd, _)) => return Err(DrainError::UnknownCommand(cmd.to_owned())),
            None => return Err(DrainError::InvalidCommand),
        }
    }
    Ok(())
}

fn fetch<W: Write, H: Handle>(id: ProjId, mut writer: W, handle: &H) -> Result<(), DrainError> {
    match handle.fetch(id.clone()) {
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
                    FetchResult::Fetched { from } => {
                        writeln!(writer, "ok: {} fetched from {}", &id, from)?;
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
    use crate::identity::ProjId;
    use crate::test;

    #[test]
    fn test_control_socket() {
        let tmp = tempfile::tempdir().unwrap();
        let handle = test::handle::Handle::default();
        let socket = tmp.path().join("alice.sock");
        let projs = test::arbitrary::set::<ProjId>(1..3);

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
            writeln!(&stream, "update {}", proj).unwrap();
        }

        let mut buf = [0; 2];
        stream.shutdown(net::Shutdown::Write).unwrap();
        stream.read_exact(&mut buf).unwrap();

        assert_eq!(&buf, &[b'o', b'k']);
        for proj in &projs {
            assert!(handle.updates.lock().unwrap().contains(proj));
        }
    }
}
