use std::net::IpAddr;

use radicle::node::{Link, NodeId};

use super::session;
use super::session::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Accept(Accept),
    Connect(Box<Connect>),
    Disconnect(Disconnect),
}

impl From<Disconnect> for Event {
    fn from(v: Disconnect) -> Self {
        Self::Disconnect(v)
    }
}

impl From<Connect> for Event {
    fn from(v: Connect) -> Self {
        Self::Connect(Box::new(v))
    }
}

impl From<Accept> for Event {
    fn from(v: Accept) -> Self {
        Self::Accept(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accept {
    LimitExceeded { ip: IpAddr, current_inbound: usize },
    IpBanned { ip: IpAddr },
    HostLimited { ip: IpAddr },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connect {
    AlreadyConnected {
        session: Session<session::Connected>,
        attempted_link: Link,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disconnect {
    AlreadyDisconnected {
        node: NodeId,
    },
    LinkConflict {
        node: NodeId,
        found: Link,
        expected: Link,
    },
}
