mod features;

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
    /// Start tracking the given node. If the node is already tracked,
    /// updates the alias if necessary.
    fn track_node(&self, id: &NodeId, alias: Option<&str>) -> Result<bool, Error>;
    /// Start tracking the given repository.
    fn track_repo(&self, id: &Id) -> Result<bool, Error>;
    /// Untrack the given node.
    fn untrack_node(&self, id: &NodeId) -> Result<bool, Error>;
    /// Untrack the given repository and delete it from storage.
    fn untrack_repo(&self, id: &Id) -> Result<bool, Error>;
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
    pub fn call<A: ToString>(
        &self,
        cmd: &str,
        args: &[A],
    ) -> Result<impl Iterator<Item = Result<String, io::Error>> + '_, io::Error> {
        let args = args
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(&self.stream, "{cmd} {args}")?;

        Ok(BufReader::new(&self.stream).lines())
    }
}

impl Handle for Node {
    fn fetch(&self, id: &Id) -> Result<(), Error> {
        for line in self.call("fetch", &[id])? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        Ok(())
    }

    fn track_node(&self, id: &NodeId, alias: Option<&str>) -> Result<bool, Error> {
        let id = id.to_human();
        let mut line = if let Some(alias) = alias {
            self.call("track-node", &[id.as_str(), alias])
        } else {
            self.call("track-node", &[id.as_str()])
        }?;
        let line = line
            .next()
            .ok_or(Error::EmptyResponse { cmd: "track-node" })??;

        log::debug!("node: {}", line);

        match line.as_str() {
            RESPONSE_OK => Ok(true),
            RESPONSE_NOOP => Ok(false),
            _ => Err(Error::InvalidResponse {
                cmd: "track-node",
                response: line,
            }),
        }
    }

    fn track_repo(&self, id: &Id) -> Result<bool, Error> {
        let mut line = self.call("track-repo", &[id])?;
        let line = line
            .next()
            .ok_or(Error::EmptyResponse { cmd: "track-repo" })??;

        log::debug!("node: {}", line);

        match line.as_str() {
            RESPONSE_OK => Ok(true),
            RESPONSE_NOOP => Ok(false),
            _ => Err(Error::InvalidResponse {
                cmd: "track-repo",
                response: line,
            }),
        }
    }

    fn untrack_node(&self, id: &NodeId) -> Result<bool, Error> {
        let mut line = self.call("untrack-node", &[id])?;
        let line = line.next().ok_or(Error::EmptyResponse {
            cmd: "untrack-node",
        })??;

        log::debug!("node: {}", line);

        match line.as_str() {
            RESPONSE_OK => Ok(true),
            RESPONSE_NOOP => Ok(false),
            _ => Err(Error::InvalidResponse {
                cmd: "untrack-node",
                response: line,
            }),
        }
    }

    fn untrack_repo(&self, id: &Id) -> Result<bool, Error> {
        let mut line = self.call("untrack-repo", &[id])?;
        let line = line.next().ok_or(Error::EmptyResponse {
            cmd: "untrack-repo",
        })??;

        log::debug!("node: {}", line);

        match line.as_str() {
            RESPONSE_OK => Ok(true),
            RESPONSE_NOOP => Ok(false),
            _ => Err(Error::InvalidResponse {
                cmd: "untrack-repo",
                response: line,
            }),
        }
    }

    fn announce_refs(&self, id: &Id) -> Result<(), Error> {
        for line in self.call("announce-refs", &[id])? {
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
