//! External effects that are emitted by interacting with [`Connections`]. These
//! effects should be used by the rest of the system to perform side-effects.
//!
//! [`Connections`]: super::Connections.

use std::net::IpAddr;

use localtime::LocalTime;
use radicle::node::{Address, Link, NodeId, Severity};

/// All effects that can occur from interacting with [`Connections`].
///
/// [`Connections`]: super::Connections.
pub enum Effect {
    Accept(Accept),
    Connect(Connect),
    Disconnect(Disconnect),
}

impl From<Accept> for Effect {
    fn from(v: Accept) -> Self {
        Self::Accept(v)
    }
}

impl From<Connect> for Effect {
    fn from(v: Connect) -> Self {
        Self::Connect(v)
    }
}

impl From<Disconnect> for Effect {
    fn from(v: Disconnect) -> Self {
        Self::Disconnect(v)
    }
}

/// Effects that occur when checking for accepting a connection from an
/// [`IpAddr`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accept {
    /// The [`IpAddr`] is likely a localhost connection.
    LocalHost { ip: IpAddr },
    /// The [`IpAddr`] should be accepted by the system, and later connected to.
    Accepted { ip: IpAddr },
}

impl Accept {
    /// The [`Accept::LocalHost`] should be created only if the [`IpAddr`] is
    /// either a loopback address or has an 'unspecified' address (see
    /// [`IpAddr::is_unspecified`]).
    pub fn local_host(ip: IpAddr) -> Option<Self> {
        (ip.is_loopback() || ip.is_unspecified()).then_some(Self::LocalHost { ip })
    }

    /// See [`Accept::Accepted`].
    pub fn accepted(ip: IpAddr) -> Self {
        Self::Accepted { ip }
    }
}

/// Effects that occur when connecting a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connect {
    /// The set of initial messages should be sent to the [`NodeId`].
    SendInitialMessages { node: NodeId, link: Link },
    /// The [`IpAddr`] of the [`NodeId`] should be recorded in an external
    /// database.
    RecordIp {
        node: NodeId,
        ip: IpAddr,
        clock: LocalTime,
    },
}

/// Effects that occur when disconnecting a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disconnect {
    /// The connection to [`NodeId`] was disconnected, but re-connection should
    /// be attempted at the given point in time.
    RetryConnection {
        /// The node that should be re-connected to.
        node: NodeId,
        /// When the node was disconnected.
        since: LocalTime,
        /// When the re-connection attempt should be made.
        retry_at: LocalTime,
    },
    /// Record the severity of the disconnect reason.
    RecordServerity {
        /// The node that was disconnected.
        node: NodeId,
        /// The address of the node that was disconnected.
        address: Address,
        /// The severity of the disconnect reason.
        severity: Severity,
    },
    /// Try to maintain all connections that are meant to be persistent.
    MaintainConnections,
}
