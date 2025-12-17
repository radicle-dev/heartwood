use std::net::IpAddr;

use localtime::{LocalDuration, LocalTime};
use radicle::node::{Link, NodeId, Severity};

use crate::connections::session;
use crate::connections::session::{ConnectionType, Pinged, Session};
use crate::service::message;

/// The result of checking an address for accepting an inbound connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accept {
    /// The inbound limit for the node has been reached.
    ///
    /// It is recommended that the incoming connection is rejected.
    LimitExceeded {
        /// The [`IpAddr`] that made the attempt.
        ip: IpAddr,
        /// The current inbound size.
        current_inbound: usize,
    },
    /// The address has been rate limited.
    ///
    /// It is recommended that the incoming connection is rejected.
    HostLimited {
        /// The [`IpAddr`] that made the attempt, and is being rate limited.
        ip: IpAddr,
    },
    /// The [`IpAddr`] is likely a localhost connection.
    ///
    /// It is recommended that this is accepted for local area networks.
    LocalHost { ip: IpAddr },
    /// The [`IpAddr`] should be accepted by the system, and the connection allowed.
    Accepted { ip: IpAddr },
}

/// The result of a connection attempt from a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attempted {
    /// The connection was transitioned to attempted.
    ConnectionAttempt {
        session: Box<Session<session::Attempted>>,
    },
    /// The session did not exist for this node, and it was expected to.
    MissingSession { node: NodeId },
    /// Attempted to connect to the local node.
    SelfConnection { node: NodeId },
}

impl Attempted {
    pub(super) fn attempt(session: Session<session::Attempted>) -> Self {
        Self::ConnectionAttempt {
            session: Box::new(session),
        }
    }

    pub(super) fn missing(node: NodeId) -> Self {
        Self::MissingSession { node }
    }
}

/// The result when making an outbound connection to a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connect {
    /// The node is already being connected to, but has not transitioned to
    /// fully-connected.
    AlreadyConnecting { node: NodeId },
    /// The node already has a connected session.
    AlreadyConnected {
        session: Box<session::Session<session::Connected>>,
    },
    /// The node is already in a disconnected state, and requires a call to
    /// reconnect to transition it to initial.
    Disconnected { node: NodeId },
    /// The caller should establish the outbound connection.
    Establish {
        /// The node to establish the connection with.
        node: NodeId,
        /// The session was given this [`ConnectionType`].
        connection_type: ConnectionType,
        /// If this is `Some`, then the [`IpAddr`] should be recorded by the
        /// local node.
        record_ip: Option<IpAddr>,
    },
    /// Attempted to connect to the local node.
    SelfConnection { node: NodeId },
}

impl Connect {
    pub(super) fn already_connecting(node: NodeId) -> Self {
        Self::AlreadyConnecting { node }
    }

    pub(super) fn already_connected(session: session::Session<session::Connected>) -> Self {
        Self::AlreadyConnected {
            session: Box::new(session),
        }
    }

    pub(super) fn disconnected(node: NodeId) -> Self {
        Self::Disconnected { node }
    }

    pub(super) fn establish(
        node: NodeId,
        connection_type: ConnectionType,
        record_ip: Option<IpAddr>,
    ) -> Self {
        Self::Establish {
            node,
            connection_type,
            record_ip,
        }
    }
}

/// The result when a node is marked a connected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connected {
    /// The connection was marked as connected.
    Established {
        session: Box<session::Session<session::Connected>>,
    },
    /// An existing session was expected for the node, but there was none.
    MissingSession { node: NodeId },
    /// Connection came from the local node.
    SelfConnection { node: NodeId },
}

impl Connected {
    pub(super) fn established(session: session::Session<session::Connected>) -> Self {
        Self::Established {
            session: Box::new(session),
        }
    }

    pub(super) fn missing(node: NodeId) -> Self {
        Self::MissingSession { node }
    }
}

/// The result when a node is disconnected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disconnected {
    /// The session was marked as disconnected and a reconnection should be
    /// tried.
    Retry {
        /// The session that is now marked as disconnected.
        session: session::Session<session::Disconnected>,
        /// The delay to wait until the reconnection.
        delay: LocalDuration,
        /// The time for when the reconnection should happen.
        retry_at: LocalTime,
    },
    /// The session was removed, and the severity of the disconnection is
    /// recorded.
    Severed {
        /// The session that was removed.
        session: session::Session<session::State>,
        /// The severity of the reason for disconnection.
        ///
        /// Can be used for penalizing the node for bad behavior.
        severity: Severity,
    },
    /// An existing session was expected for the node, but there was none.
    MissingSession { node: NodeId },
    /// The node was already marked as disconnected.
    AlreadyDisconnected { node: NodeId },
    /// The reported link of the disconnect did not match the existing link of
    /// the session.
    LinkConflict {
        node: NodeId,
        /// The link that was found in the existing session.
        found: Link,
        /// The link that was expected from the call to disconnect.
        expected: Link,
    },
    /// Attempted to disconnect from the local node.
    SelfConnection { node: NodeId },
}

impl Disconnected {
    pub(super) fn retry(
        session: session::Session<session::Disconnected>,
        delay: LocalDuration,
        retry_at: LocalTime,
    ) -> Self {
        Self::Retry {
            session,
            delay,
            retry_at,
        }
    }

    pub(super) fn severed(session: session::Session<session::State>, severity: Severity) -> Self {
        Self::Severed { session, severity }
    }

    pub(super) fn already_disconnected(node: NodeId) -> Self {
        Self::AlreadyDisconnected { node }
    }

    pub(super) fn conflict<S>(session: &session::Session<S>, expected: Link) -> Self {
        Self::LinkConflict {
            node: session.node(),
            found: *session.link(),
            expected,
        }
    }

    pub(super) fn missing(node: NodeId) -> Self {
        Self::MissingSession { node }
    }
}

/// The result when a node is being reconnected to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reconnect {
    /// The connection was marked as initial, transitioning from a disconnected
    /// state.
    Reconnecting {
        session: Box<session::Session<session::Initial>>,
    },
    /// An existing session was expected for the node, but there was none.
    MissingSession { node: NodeId },
    /// Attempted to reconnect to the local node.
    SelfConnection { node: NodeId },
}

impl Reconnect {
    pub(super) fn reconnecting(session: session::Session<session::Initial>) -> Self {
        Self::Reconnecting {
            session: Box::new(session),
        }
    }

    pub(super) fn missing(node: NodeId) -> Self {
        Self::MissingSession { node }
    }
}

/// The result of handling an incoming message from a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandledMessage {
    /// The node was in a disconnected state, so the message was dropped.
    Disconnected { node: NodeId },
    /// The node was rate limited, so the message was dropped.
    RateLimited { node: NodeId },
    /// The node's subscription was updated, and is in a connected state.
    Subscribed {
        session: session::Session<session::Connected>,
    },
    /// The node's pong was received, and is in a connected state.
    Pinged {
        session: session::Session<session::Connected>,
        pinged: Option<Pinged>,
    },
    /// There was no message to process, and the node is in a connected state.
    Connected {
        session: session::Session<session::Connected>,
    },
    /// An existing session was expected for the node, but there was none.
    MissingSession { node: NodeId },
    /// Message originated from the local node.
    SelfConnection { node: NodeId },
}

/// The result of pinging a connected session.
pub struct Ping {
    /// The session that was being pinged.
    pub session: session::Session<session::Connected>,
    /// The ping message.
    pub ping: message::Ping,
}
