mod features;

use std::fmt;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::crypto::PublicKey;
use crate::identity::Id;

pub use features::Features;

/// Default name for control socket file.
pub const DEFAULT_SOCKET_NAME: &str = "radicle.sock";
/// Response on node socket indicating that a command was carried out successfully.
pub const RESPONSE_OK: &str = "ok";
/// Response on node socket indicating that a command had no effect.
pub const RESPONSE_NOOP: &str = "noop";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to connect to node: {0}")]
    Connect(#[from] io::Error),
    #[error("received invalid response for `{cmd}` command: '{response}'")]
    InvalidResponse { cmd: &'static str, response: String },
    #[error("received empty response for `{cmd}` command")]
    EmptyResponse { cmd: &'static str },
}

pub trait Handle {
    /// Fetch a project from the network. Fails if the project isn't tracked.
    fn fetch(&self, id: &Id) -> Result<(), Error>;
    /// Start tracking the given project. Doesn't do anything if the project is already
    /// tracked.
    fn track(&self, id: &Id) -> Result<bool, Error>;
    /// Untrack the given project and delete it from storage.
    fn untrack(&self, id: &Id) -> Result<bool, Error>;
    /// Notify the network that we have new refs.
    fn announce_refs(&self, id: &Id) -> Result<(), Error>;
    /// Ask the node to shutdown.
    fn shutdown(self) -> Result<(), Error>;
}

/// Public node & device identifier.
pub type NodeId = PublicKey;

/// Node controller.
#[derive(Debug)]
pub struct Node {
    stream: UnixStream,
}

impl Node {
    /// Connect to the node, via the socket at the given path.
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let stream = UnixStream::connect(path).map_err(Error::Connect)?;

        Ok(Self { stream })
    }

    /// Call a command on the node.
    pub fn call<A: fmt::Display>(
        &self,
        cmd: &str,
        arg: &A,
    ) -> Result<impl Iterator<Item = Result<String, io::Error>> + '_, io::Error> {
        writeln!(&self.stream, "{cmd} {arg}")?;

        Ok(BufReader::new(&self.stream).lines())
    }
}

impl Handle for Node {
    fn fetch(&self, id: &Id) -> Result<(), Error> {
        for line in self.call("fetch", id)? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        Ok(())
    }

    fn track(&self, id: &Id) -> Result<bool, Error> {
        let mut line = self.call("track", id)?;
        let line = line.next().ok_or(Error::EmptyResponse { cmd: "track" })??;

        log::debug!("node: {}", line);

        match line.as_str() {
            RESPONSE_OK => Ok(true),
            RESPONSE_NOOP => Ok(false),
            _ => Err(Error::InvalidResponse {
                cmd: "track",
                response: line,
            }),
        }
    }

    fn untrack(&self, id: &Id) -> Result<bool, Error> {
        let mut line = self.call("untrack", id)?;
        let line = line
            .next()
            .ok_or(Error::EmptyResponse { cmd: "untrack" })??;

        log::debug!("node: {}", line);

        match line.as_str() {
            RESPONSE_OK => Ok(true),
            RESPONSE_NOOP => Ok(false),
            _ => Err(Error::InvalidResponse {
                cmd: "untrack",
                response: line,
            }),
        }
    }

    fn announce_refs(&self, id: &Id) -> Result<(), Error> {
        for line in self.call("announce-refs", id)? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        Ok(())
    }

    fn shutdown(self) -> Result<(), Error> {
        todo!();
    }
}

/// Connect to the local node.
pub fn connect<P: AsRef<Path>>(path: P) -> Result<Node, Error> {
    Node::connect(path)
}
