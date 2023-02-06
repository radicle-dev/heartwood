mod features;

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::{fmt, io, net};

use amplify::WrapperMut;
use crossbeam_channel as chan;
use cyphernet::addr::{HostName, NetAddr};
use serde::de::DeserializeOwned;
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

/// Result of a command, on the node control socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum CommandResult {
    /// Response on node socket indicating that a command was carried out successfully.
    #[serde(rename = "ok")]
    Okay {
        /// Whether the command had any effect.
        #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
        updated: bool,
    },
    /// Response on node socket indicating that an error occured.
    Error {
        /// The reason for the error.
        reason: String,
    },
}

impl CommandResult {
    /// Create an "updated" response.
    pub fn updated() -> Self {
        Self::Okay { updated: true }
    }

    /// Create an "ok" response.
    pub fn ok() -> Self {
        Self::Okay { updated: false }
    }

    /// Create an error result.
    pub fn error(err: impl std::error::Error) -> Self {
        Self::Error {
            reason: err.to_string(),
        }
    }

    /// Write this command result to a stream, including a terminating LF character.
    pub fn to_writer(&self, mut w: impl io::Write) -> io::Result<()> {
        json::to_writer(&mut w, self).map_err(|_| io::ErrorKind::InvalidInput)?;
        w.write_all(b"\n")
    }
}

impl From<CommandResult> for Result<bool, Error> {
    fn from(value: CommandResult) -> Self {
        match value {
            CommandResult::Okay { updated } => Ok(updated),
            CommandResult::Error { reason } => Err(Error::Node(reason)),
        }
    }
}

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

/// Command name.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandName {
    /// Announce repository references for given repository to peers.
    AnnounceRefs,
    /// Connect to node with the given address.
    Connect,
    /// Lookup seeds for the given repository in the routing table.
    Seeds,
    /// Fetch the given repository from the network.
    Fetch,
    /// Track the given repository.
    TrackRepo,
    /// Untrack the given repository.
    UntrackRepo,
    /// Track the given node.
    TrackNode,
    /// Untrack the given node.
    UntrackNode,
    /// Get the node's inventory.
    Inventory,
    /// Get the node's routing table.
    Routing,
    /// Get the node's status.
    Status,
    /// Shutdown the node.
    Shutdown,
}

impl fmt::Display for CommandName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: The enum can always be converted to a value.
        #[allow(clippy::unwrap_used)]
        let val = json::to_value(self).unwrap();
        // SAFETY: The value is always a string.
        #[allow(clippy::unwrap_used)]
        let s = val.as_str().unwrap();

        write!(f, "{s}")
    }
}

/// Commands sent to the node via the control socket.
#[derive(Debug, Serialize, Deserialize)]
pub struct Command {
    /// Command name.
    #[serde(rename = "cmd")]
    pub name: CommandName,
    /// Command arguments.
    #[serde(rename = "args")]
    pub args: Vec<String>,
}

impl Command {
    /// Shutdown command.
    pub const SHUTDOWN: Self = Self {
        name: CommandName::Shutdown,
        args: vec![],
    };

    /// Create a new command.
    pub fn new<T: ToString>(name: CommandName, args: impl IntoIterator<Item = T>) -> Self {
        Self {
            name,
            args: args.into_iter().map(|a| a.to_string()).collect(),
        }
    }

    /// Write this command to a stream, including a terminating LF character.
    pub fn to_writer(&self, mut w: impl io::Write) -> io::Result<()> {
        json::to_writer(&mut w, self).map_err(|_| io::ErrorKind::InvalidInput)?;
        w.write_all(b"\n")
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

/// Error returned by [`Handle`] functions.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to connect to node: {0}")]
    Connect(#[from] io::Error),
    #[error("failed to call node: {0}")]
    Call(#[from] CallError),
    #[error("node: {0}")]
    Node(String),
    #[error("received empty response for `{cmd}` command")]
    EmptyResponse { cmd: CommandName },
}

/// Error returned by [`Node::call`] iterator.
#[derive(thiserror::Error, Debug)]
pub enum CallError {
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("received invalid json in response for `{cmd}` command: '{response}': {error}")]
    InvalidJson {
        cmd: CommandName,
        response: String,
        error: json::Error,
    },
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
    pub fn call<A: ToString, T: DeserializeOwned>(
        &self,
        name: CommandName,
        args: impl IntoIterator<Item = A>,
    ) -> Result<impl Iterator<Item = Result<T, CallError>>, io::Error> {
        let stream = UnixStream::connect(&self.socket)?;
        Command::new(name, args).to_writer(&stream)?;

        Ok(BufReader::new(stream).lines().map(move |l| {
            let l = l?;
            let v = json::from_str(&l).map_err(|e| CallError::InvalidJson {
                cmd: name,
                response: l,
                error: e,
            })?;

            Ok(v)
        }))
    }
}

impl Handle for Node {
    type Sessions = ();
    type Error = Error;
    type FetchResult = FetchResult;

    fn is_running(&self) -> bool {
        let Ok(mut lines) = self.call::<&str, CommandResult>(CommandName::Status, []) else {
            return false;
        };
        let Some(Ok(result)) = lines.next() else {
            return false;
        };
        matches!(result, CommandResult::Okay { .. })
    }

    fn connect(&mut self, _node: NodeId, _addr: Address) -> Result<(), Error> {
        todo!()
    }

    fn seeds(&mut self, id: Id) -> Result<Vec<NodeId>, Error> {
        let seeds: Vec<NodeId> =
            self.call(CommandName::Seeds, [id.urn()])?
                .next()
                .ok_or(Error::EmptyResponse {
                    cmd: CommandName::Seeds,
                })??;

        Ok(seeds)
    }

    fn fetch(&mut self, id: Id, from: NodeId) -> Result<Self::FetchResult, Error> {
        let result = self
            .call(CommandName::Fetch, [id.urn(), from.to_human()])?
            .next()
            .ok_or(Error::EmptyResponse {
                cmd: CommandName::Fetch,
            })??;

        Ok(result)
    }

    fn track_node(&mut self, id: NodeId, alias: Option<String>) -> Result<bool, Error> {
        let id = id.to_human();
        let args = if let Some(alias) = alias.as_deref() {
            vec![id.as_str(), alias]
        } else {
            vec![id.as_str()]
        };

        let mut line = self.call(CommandName::TrackNode, args)?;
        let response: CommandResult = line.next().ok_or(Error::EmptyResponse {
            cmd: CommandName::TrackNode,
        })??;

        response.into()
    }

    fn track_repo(&mut self, id: Id) -> Result<bool, Error> {
        let mut line = self.call(CommandName::TrackRepo, [id.urn()])?;
        let response: CommandResult = line.next().ok_or(Error::EmptyResponse {
            cmd: CommandName::TrackRepo,
        })??;

        response.into()
    }

    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Error> {
        let mut line = self.call(CommandName::UntrackNode, [id])?;
        let response: CommandResult = line.next().ok_or(Error::EmptyResponse {
            cmd: CommandName::UntrackNode,
        })??;

        response.into()
    }

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Error> {
        let mut line = self.call(CommandName::UntrackRepo, [id.urn()])?;
        let response: CommandResult = line.next().ok_or(Error::EmptyResponse {
            cmd: CommandName::UntrackRepo,
        })??;

        response.into()
    }

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        for line in self.call(CommandName::AnnounceRefs, [id.urn()])? {
            line?;
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_command_name_display() {
        assert_eq!(CommandName::TrackNode.to_string(), "track-node");
    }
}
