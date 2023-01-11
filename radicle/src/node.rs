mod features;

use amplify::WrapperMut;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::{io, net};

use cyphernet::addr::{HostName, NetAddr};

use crate::crypto::PublicKey;
use crate::identity::Id;
use crossbeam_channel as chan;

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

impl cyphernet::addr::Host for Address {}
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
    /// The result of a fetch request.
    type FetchLookup;
    /// The peer session type.
    type Session;
    /// The error returned by all methods.
    type Error: std::error::Error;

    /// Connect to a peer.
    fn connect(&mut self, node: NodeId, addr: Address) -> Result<(), Self::Error>;
    /// Retrieve or update the project from network.
    fn fetch(&mut self, id: Id) -> Result<Self::FetchLookup, Self::Error>;
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
    fn sessions(&self) -> Result<chan::Receiver<(NodeId, Self::Session)>, Self::Error>;
    /// Query the inventory.
    fn inventory(&self) -> Result<chan::Receiver<Id>, Self::Error>;
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
    type Session = ();
    type FetchLookup = ();
    type Error = Error;

    fn connect(&mut self, _node: NodeId, _addr: Address) -> Result<(), Error> {
        todo!()
    }

    fn fetch(&mut self, id: Id) -> Result<(), Error> {
        for line in self.call("fetch", &[id])? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        Ok(())
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

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        for line in self.call("announce-refs", &[id])? {
            let line = line?;
            log::debug!("node: {}", line);
        }
        Ok(())
    }

    fn routing(&self) -> Result<chan::Receiver<(Id, NodeId)>, Error> {
        todo!();
    }

    fn sessions(&self) -> Result<chan::Receiver<(NodeId, Self::Session)>, Error> {
        todo!();
    }

    fn inventory(&self) -> Result<chan::Receiver<Id>, Error> {
        todo!();
    }

    fn shutdown(self) -> Result<(), Error> {
        todo!();
    }
}

/// Connect to the local node.
pub fn connect<P: AsRef<Path>>(path: P) -> Result<Node, Error> {
    Node::connect(path)
}
