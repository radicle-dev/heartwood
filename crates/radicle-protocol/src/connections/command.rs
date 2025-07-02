use localtime::LocalTime;

use radicle::node::{Address, Link, NodeId};

use crate::service::DisconnectReason;

pub enum Connect {
    Inbound(Inbound),
    Outbound(Outbound),
}

impl Connect {
    pub fn inbound(node: NodeId, clock: LocalTime, persistent: bool) -> Self {
        Self::Inbound(Inbound {
            node,
            clock,
            persistent,
        })
    }

    pub fn outbound(node: NodeId, addr: Address, persistent: bool, clock: LocalTime) -> Self {
        Self::Outbound(Outbound {
            node,
            addr,
            persistent,
            clock,
        })
    }
}

pub struct Inbound {
    pub(super) node: NodeId,
    pub(super) clock: LocalTime,
    pub(super) persistent: bool,
}

pub struct Outbound {
    pub(super) node: NodeId,
    pub(super) addr: Address,
    pub(super) persistent: bool,
    pub(super) clock: LocalTime,
}

pub struct Disconnect {
    pub(super) node: NodeId,
    pub(super) link: Link,
    pub(super) reason: DisconnectReason,
    pub(super) since: LocalTime,
}
