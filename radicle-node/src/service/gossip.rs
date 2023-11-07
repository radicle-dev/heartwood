pub mod store;

use super::*;
use crate::service::filter::Filter;

pub use store::Error;
pub use store::GossipStore as Store;

pub fn handshake<G: Signer, S: ReadStorage>(
    node: NodeAnnouncement,
    now: Timestamp,
    storage: &S,
    signer: &G,
    filter: Filter,
) -> Vec<Message> {
    let inventory = match storage.inventory() {
        Ok(i) => i,
        Err(e) => {
            error!("Error getting local inventory for handshake: {}", e);
            // Other than crashing the node completely, there's nothing we can do
            // here besides returning an empty inventory and logging an error.
            vec![]
        }
    };

    vec![
        Message::node(node, signer),
        Message::inventory(gossip::inventory(now, inventory), signer),
        Message::subscribe(
            filter,
            now - SUBSCRIBE_BACKLOG_DELTA.as_millis() as u64,
            Timestamp::MAX,
        ),
    ]
}

pub fn node(config: &Config, timestamp: Timestamp) -> NodeAnnouncement {
    let features = config.features();
    let alias = config.alias.clone();
    let addresses: BoundedVec<_, ADDRESS_LIMIT> = config
        .external_addresses
        .clone()
        .try_into()
        .expect("external addresses are within the limit");

    NodeAnnouncement {
        features,
        timestamp,
        alias,
        addresses,
        nonce: 0,
    }
}

pub fn inventory(timestamp: Timestamp, inventory: Vec<Id>) -> InventoryAnnouncement {
    type Inventory = BoundedVec<Id, INVENTORY_LIMIT>;

    if inventory.len() > Inventory::max() {
        error!(
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
