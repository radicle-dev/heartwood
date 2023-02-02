mod features;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{io, net};

use amplify::WrapperMut;
use crossbeam_channel as chan;
use cyphernet::addr::{HostName, NetAddr};
use serde::{Deserialize, Serialize};
use serde_json as json;

use crate::crypto::PublicKey;
use crate::identity::Id;
use crate::storage::RefUpdate;

pub use features::Features;

/// Default name for control socket file.
pub const DEFAULT_SOCKET_NAME: &str = "radicle.sock";
/// Default radicle protocol port.
pub const DEFAULT_PORT: u16 = 8776;
/// Response on node socket indicating that a command was carried out successfully.
pub const RESPONSE_OK: &str = "ok";
/// Response on node socket indicating that a command had no effect.
pub const RESPONSE_NOOP: &str = "noop";

/// Peer public protocol address.
#[derive(Wrapper, WrapperMut, Clone, Eq, PartialEq, Debug, From)]
#[wrapper(Deref, Display, FromStr)]
#[wrapper_mut(DerefMut)]
pub struct Address(NetAddr<HostName>);

impl cyphernet::addr::Host for Address {
    fn requires_proxy(&self) -> bool {
        self.0.requires_proxy()
    }
}

impl cyphernet::addr::Addr for Address {
    fn port(&self) -> u16 {
        self.0.port()
    }
}

impl From<net::SocketAddr> for Address {
    fn from(addr: net::SocketAddr) -> Self {
        Address(NetAddr {
            host: HostName::Ip(addr.ip()),
            port: addr.port(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum FetchResult {
    Success { updated: Vec<RefUpdate> },
    Failed { reason: String },
}

impl FetchResult {
    pub fn is_success(&self) -> bool {
        matches!(self, FetchResult::Success { .. })
    }

    pub fn success(self) -> Option<Vec<RefUpdate>> {
        match self {
            Self::Success { updated } => Some(updated),
            _ => None,
        }
    }
}

impl<S: ToString> From<Result<Vec<RefUpdate>, S>> for FetchResult {
    fn from(value: Result<Vec<RefUpdate>, S>) -> Self {
        match value {
            Ok(updated) => Self::Success { updated },
            Err(err) => Self::Failed {
                reason: err.to_string(),
            },
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to connect to node: {0}")]
    Connect(#[from] io::Error),
    #[error("received invalid response for `{cmd}` command: '{response}'")]
    InvalidResponse { cmd: &'static str, response: String },
    #[error("received invalid json in response for `{cmd}` command: '{response}': {error}")]
    InvalidJson {
        cmd: &'static str,
        response: String,
        error: json::Error,
    },
    #[error("received empty response for `{cmd}` command")]
    EmptyResponse { cmd: &'static str },
}

/// A handle to send commands to the node or request information.
pub trait Handle {
    /// The peer sessions type.
    type Sessions;
    /// The error returned by all methods.
    type Error: std::error::Error + Send + Sync + 'static;
    /// Result of a fetch.
    type FetchResult;

    /// Check if the node is running. to a peer.
    fn is_running(&self) -> bool;
    /// Connect to a peer.
    fn connect(&mut self, node: NodeId, addr: Address) -> Result<(), Self::Error>;
    /// Lookup the seeds of a given repository in the routing table.
    fn seeds(&mut self, id: Id) -> Result<Vec<NodeId>, Self::Error>;
    /// Fetch a repository from the network.
    fn fetch(&mut self, id: Id, from: NodeId) -> Result<Self::FetchResult, Self::Error>;
    /// Start tracking the given project. Doesn't do anything if the project is already
    /// tracked.
    fn track_repo(&mut self, id: Id) -> Result<bool, Self::Error>;
    /// Start tracking the given node.
    fn track_node(&mut self, id: NodeId, alias: Option<String>) -> Result<bool, Self::Error>;
    /// Untrack the given project and delete it from storage.
    fn untrack_repo(&mut self, id: Id) -> Result<bool, Self::Error>;
    /// Untrack the given node.
    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Self::Error>;
    /// Notify the client that a project has been updated.
    fn announce_refs(&mut self, id: Id) -> Result<(), Self::Error>;
    /// Ask the client to shutdown.
    fn shutdown(self) -> Result<(), Self::Error>;
    /// Query the routing table entries.
    fn routing(&self) -> Result<chan::Receiver<(Id, NodeId)>, Self::Error>;
    /// Query the peer session state.
    fn sessions(&self) -> Result<Self::Sessions, Self::Error>;
    /// Query the inventory.
    fn inventory(&self) -> Result<chan::Receiver<Id>, Self::Error>;
}

/// Public node & device identifier.
pub type NodeId = PublicKey;

/// Node controller.
#[derive(Debug)]
pub struct Node {
    socket: PathBuf,
}

impl Node {
    /// Connect to the node, via the socket at the given path.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            socket: path.as_ref().to_path_buf(),
        }
    }

    /// Call a command on the node.
    pub fn call<A: ToString>(
        &self,
        cmd: &str,
        args: &[A],
    ) -> Result<impl Iterator<Item = Result<String, io::Error>>, io::Error> {
        let stream = UnixStream::connect(&self.socket)?;
        let args = args
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");

        if args.is_empty() {
            writeln!(&stream, "{cmd}")?;
        } else {
            writeln!(&stream, "{cmd} {args}")?;
        }
        Ok(BufReader::new(stream).lines())
    }
}

impl Handle for Node {
    type Sessions = ();
    type Error = Error;
    type FetchResult = FetchResult;

    fn is_running(&self) -> bool {
        let Ok(mut lines) = self.call::<&str>("status", &[]) else {
            return false;
        };
        let Some(Ok(line)) = lines.next() else {
            return false;
        };
        line == RESPONSE_OK
    }

    fn connect(&mut self, _node: NodeId, _addr: Address) -> Result<(), Error> {
        todo!()
    }

    fn seeds(&mut self, id: Id) -> Result<Vec<NodeId>, Error> {
        self.call("seeds", &[id.urn()])?
            .map(|line| {
                let line = line?;
                let node = NodeId::from_str(&line).map_err(|_| Error::InvalidResponse {
                    cmd: "seeds",
                    response: line,
                })?;
                Ok(node)
            })
            .collect()
    }

    fn fetch(&mut self, id: Id, from: NodeId) -> Result<Self::FetchResult, Error> {
        let result = self
            .call("fetch", &[id.urn(), from.to_human()])?
            .next()
            .ok_or(Error::EmptyResponse { cmd: "fetch" })??;
        let lookup = json::from_str(&result).map_err(|e| Error::InvalidJson {
            cmd: "fetch",
            response: result,
            error: e,
        })?;

        Ok(lookup)
    }

    fn track_node(&mut self, id: NodeId, alias: Option<String>) -> Result<bool, Error> {
        let id = id.to_human();
        let mut line = if let Some(alias) = alias.as_deref() {
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

    fn track_repo(&mut self, id: Id) -> Result<bool, Error> {
        let mut line = self.call("track-repo", &[id.urn()])?;
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

    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Error> {
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

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Error> {
        let mut line = self.call("untrack-repo", &[id.urn()])?;
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

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        for line in self.call("announce-refs", &[id.urn()])? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        Ok(())
    }

    fn routing(&self) -> Result<chan::Receiver<(Id, NodeId)>, Error> {
        todo!();
    }

    fn sessions(&self) -> Result<Self::Sessions, Error> {
        todo!();
    }

    fn inventory(&self) -> Result<chan::Receiver<Id>, Error> {
        todo!();
    }

    fn shutdown(self) -> Result<(), Error> {
        todo!();
    }
}
