pub mod store;

use std::str::FromStr;
use std::sync::LazyLock;

use super::*;
use crate::{PROTOCOL_VERSION, VERSION};
use radicle::node::UserAgent;

pub use store::{AnnouncementId, Error, RelayStatus, Store};

/// This node's user agent string.
pub static USER_AGENT: LazyLock<UserAgent> = LazyLock::new(|| {
    FromStr::from_str(format!("/radicle:{}/", VERSION.version).as_str())
        .expect("user agent is valid")
});

pub fn node(config: &Config, timestamp: Timestamp) -> NodeAnnouncement {
    let features = config.features();
    let alias = config.alias.clone();
    let addresses: BoundedVec<_, ADDRESS_LIMIT> = config
        .external_addresses
        .clone()
        .try_into()
        .expect("external addresses are within the limit");
    let agent = USER_AGENT.clone();
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
