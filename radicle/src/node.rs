mod features;

use amplify::WrapperMut;
use std::io::{BufRead, BufReader, Write};
use std::ops::Deref;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::{io, net};

use crossbeam_channel as chan;
use cyphernet::addr::{HostName, NetAddr};
use nonempty::NonEmpty;

use crate::crypto::PublicKey;
use crate::git;
use crate::identity::Id;
use crate::storage;
use crate::storage::{Namespaces, RefUpdate};

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

/// Result of a fetch request from a specific seed.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub struct FetchResult {
    pub rid: Id,
    pub remote: NodeId,
    pub namespaces: Namespaces,
    pub result: Result<Vec<RefUpdate>, FetchError>,
}

impl Deref for FetchResult {
    type Target = Result<Vec<RefUpdate>, FetchError>;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

/// Error returned by fetch.
#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Fetch(#[from] storage::FetchError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Project(#[from] storage::ProjectError),
}

/// Result of looking up seeds in our routing table.
/// This object is sent back to the caller who initiated the fetch.
#[derive(Debug)]
pub enum FetchLookup {
    /// Found seeds for the given project.
    Found {
        seeds: NonEmpty<NodeId>,
        results: chan::Receiver<FetchResult>,
    },
    /// Can't fetch because no seeds were found for this project.
    NotFound,
    /// Can't fetch because the project isn't tracked.
    NotTracking,
    /// Error trying to find seeds.
    Error(FetchError),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to connect to node: {0}")]
    Connect(#[from] io::Error),
    #[error("received invalid response for `{cmd}` command: '{response}'")]
    InvalidResponse { cmd: &'static str, response: String },
    #[error("received empty response for `{cmd}` command")]
    EmptyResponse { cmd: &'static str },
}

/// A handle to send commands to the node or request information.
pub trait Handle {
    /// The peer sessions type.
    type Sessions;
    /// The error returned by all methods.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Check if the node is running. to a peer.
    fn is_running(&self) -> bool;
    /// Connect to a peer.
    fn connect(&mut self, node: NodeId, addr: Address) -> Result<(), Self::Error>;
    /// Retrieve or update the project from network.
    fn fetch(&mut self, id: Id) -> Result<FetchLookup, Self::Error>;
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

    fn fetch(&mut self, id: Id) -> Result<FetchLookup, Error> {
        for line in self.call("fetch", &[id.urn()])? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        // TODO: Return parsed lookup results.
        Ok(FetchLookup::NotFound)
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
