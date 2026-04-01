pub mod store;

use super::*;
use crate::bounded::BoundedVec;
use radicle::node::PROTOCOL_VERSION;

pub use store::{AnnouncementId, Error, RelayStatus, Store};

pub fn node(config: &Config, timestamp: Timestamp) -> NodeAnnouncement {
    let features = config.features();
    let alias = config.alias.clone();
    let addresses: BoundedVec<_, ADDRESS_LIMIT> = config
        .external_addresses
        .clone()
        .try_into()
        .expect("external addresses are within the limit");

    let agent = config.user_agent();

    let version = PROTOCOL_VERSION;

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

pub fn inventory(
    timestamp: Timestamp,
    inventory: impl IntoIterator<Item = RepoId>,
) -> InventoryAnnouncement {
    let inventory = inventory.into_iter().collect::<Vec<_>>();
    if inventory.len() > INVENTORY_LIMIT {
        warn!(
            target: "service",
            "inventory announcement limit ({}) exceeded, other nodes will see only some of your projects",
            inventory.len()
        );
    }

    InventoryAnnouncement {
        inventory: BoundedVec::truncate(inventory),
        timestamp,
    }
}
