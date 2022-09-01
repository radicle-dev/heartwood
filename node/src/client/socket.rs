use std::io::prelude::*;
use std::io::BufReader;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::str::FromStr;
use std::{fs, io, net};

use nakamoto_net::Reactor;

use crate::client;
use crate::client::handle::Handle;
use crate::identity::ProjId;

/// Default name for control socket file.
pub const DEFAULT_NAME: &str = "radicle.sock";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to bind control socket listener: {0}")]
    Bind(io::Error),
}

/// Listen for commands on the control socket, and process them.
pub fn listen<P: AsRef<Path>, R: Reactor>(path: P, handle: Handle<R>) -> Result<(), Error> {
    // Remove the socket file on startup before rebinding.
    fs::remove_file(&path).ok();

    let listener = UnixListener::bind(path).map_err(Error::Bind)?;
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(e) = drain(&stream, &handle) {
                    log::error!("Received {} on control socket", e);

                    write!(stream, "error: {}", e).ok();

                    stream.flush().ok();
                    stream.shutdown(net::Shutdown::Both).ok();
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
}

fn drain<R: Reactor>(stream: &UnixStream, handle: &Handle<R>) -> Result<(), DrainError> {
    let mut reader = BufReader::new(stream);

    for line in reader.by_ref().lines().flatten() {
        match line.split_once(' ') {
            Some(("update", arg)) => {
                if let Ok(id) = ProjId::from_str(arg) {
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
