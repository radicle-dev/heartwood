use std::net::IpAddr;

use localtime::LocalTime;
use radicle::node::{Address, Link, NodeId};

use crate::connections::session;
use crate::connections::session::ConnectionType;
use crate::service::ZeroBytes;
use crate::service::message;

/// Check whether the incoming [`IpAddr`] should be accepted for connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accept {
    pub ip: IpAddr,
}

/// Mark a connection as attempted.
pub struct Attempt {
    /// The node that is being attempted.
    pub node: NodeId,
}

/// Make an outbound connection to a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connect {
    /// The node that is being connected to.
    pub node: NodeId,
    /// The found address of the node that is being contacted.
    pub addr: Address,
    /// Mark the session with the given [`ConnectionType`].
    pub connection_type: ConnectionType,
}

/// Mark the node as connected.
pub enum Connected {
    /// The connected node is made through an inbound connection.
    Inbound {
        /// The node that is now connected.
        node: NodeId,
        /// The address the node is connected via.
        addr: Address,
        /// Mark the session with the given [`ConnectionType`].
        connection_type: ConnectionType,
    },
    /// The connected node is made through an outbound connection.
    Outbound {
        /// The node that is now connected.
        node: NodeId,
        /// The address the node is connected via.
        addr: Address,
        /// Mark the session with the given [`ConnectionType`].
        connection_type: ConnectionType,
    },
}

/// Either mark the node as disconnected, or remove its session.
#[derive(Debug)]
pub struct Disconnect {
    /// The node being disconnected.
    pub node: NodeId,
    /// The link of the disconnection.
    pub link: Link,
    /// When did the disconnection occur.
    pub since: LocalTime,
    /// Decides whether the session is disconnected or removed.
    pub connection_type: ConnectionType,
}

/// Mark the node as initial, if it was disconnected.
#[derive(Debug)]
pub struct Reconnect {
    pub node: NodeId,
}

/// Handle an incoming message from the given node.
pub struct Message {
    /// The node sending the message.
    pub node: NodeId,
    /// The payload that is part of the incoming message.
    ///
    /// Not all messages are required for changing the state of the connection's
    /// state, so it is optional.
    pub payload: Option<Payload>,
    /// Mark the session with the given [`ConnectionType`].
    pub connection_type: ConnectionType,
}

/// The payload of an incoming message.
pub enum Payload {
    /// The message describes the node's subscription payload.
    Subscribe(message::Subscribe),
    /// The message was a "pong" in response to this node's "ping".
    Pong(session::Pong),
}

impl Payload {
    pub fn pong(zeroes: ZeroBytes, now: LocalTime) -> Self {
        Self::Pong(session::Pong { now, zeroes })
    }

    pub fn subscribe(subscription: message::Subscribe) -> Self {
        Self::Subscribe(subscription)
    }
}
