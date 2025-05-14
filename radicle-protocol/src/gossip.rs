//! Gossip protocol logic

use crate::prelude::BoundedVec;
use radicle::identity::RepoId;
use radicle::node::{Alias, Features, Timestamp, UserAgent};
use radicle::storage::refs::RefsAt;

use crate::message::{InventoryAnnouncement, NodeAnnouncement, RefsAnnouncement, ADDRESS_LIMIT};

/// Direction of network link.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Link {
    /// Inbound link.
    Inbound,
    /// Outbound link.
    Outbound,
}

impl Link {
    /// Check if this link is inbound.
    pub fn is_inbound(&self) -> bool {
        matches!(self, Self::Inbound)
    }

    /// Check if this link is outbound.
    pub fn is_outbound(&self) -> bool {
        matches!(self, Self::Outbound)
    }
}

/// Create a node announcement message
pub fn node(
    features: Features,
    timestamp: Timestamp,
    alias: Alias,
    addresses: Vec<radicle::node::Address>,
    agent: UserAgent,
    version: u8,
) -> NodeAnnouncement {
    let addresses: BoundedVec<_, ADDRESS_LIMIT> = addresses
        .try_into()
        .expect("external addresses are within the limit");

    NodeAnnouncement {
        features,
        version,
        timestamp,
        alias,
        addresses,
        nonce: 0,
        agent,
    }
}

/// Create an inventory announcement
pub fn inventory(
    timestamp: Timestamp,
    inventory: impl IntoIterator<Item = RepoId>,
) -> InventoryAnnouncement {
    let inventory = inventory.into_iter().collect::<Vec<_>>();

    InventoryAnnouncement {
        inventory: BoundedVec::truncate(inventory),
        timestamp,
    }
}

/// Create a refs announcement
pub fn refs(rid: RepoId, timestamp: Timestamp, refs: Vec<RefsAt>) -> RefsAnnouncement {
    RefsAnnouncement {
        rid,
        refs: BoundedVec::truncate(refs),
        timestamp,
    }
}
