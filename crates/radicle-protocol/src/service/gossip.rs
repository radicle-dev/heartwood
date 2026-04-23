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

    #[cfg(not(feature = "unstable-sha256"))]
    let inventory = {
        if inventory.len() > INVENTORY_LIMIT_SHA1 {
            warn!(
                target: "service",
                "inventory announcement limit ({}) exceeded, other nodes will see only some of your projects",
                inventory.len()
            );
        }

        BoundedVec::truncate(inventory)
    };

    #[cfg(feature = "unstable-sha256")]
    let inventory = {
        use crate::service::message::INVENTORY_LIMIT_BYTES;

        // Decreasing counter, which tracks the remaining capacity of the
        // inventory announcement in bytes.
        let mut remaining = INVENTORY_LIMIT_BYTES;

        // Increasing counter which tracks the number of repositories that
        // are included in the inventory announcement.
        let mut count = 0;

        let mut result = BoundedVec::new();
        let mut iter = inventory.into_iter();

        while let Some(rid) = iter.next() {
            let len = 2 + rid.len();

            if len <= remaining {
                result.push(rid).expect("RID fits");
                remaining -= len;
                count += 1;
                continue;
            }

            let hint = match iter.size_hint() {
                (lower, Some(upper)) if lower == upper => lower.to_string(),
                (lower, Some(upper)) => format!("between {} and {}", lower, upper),
                (lower, None) if lower != 0 => format!("at least {}", lower),
                _ => "some".to_string(),
            };

            warn!(
                "Inventory size exceeds announcement limit. {count} repositories will be announced, while {hint} will not be."
            );

            break;
        }

        result
    };

    InventoryAnnouncement {
        timestamp,
        inventory,
    }
}
