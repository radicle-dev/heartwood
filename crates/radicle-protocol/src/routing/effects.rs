use std::collections::BTreeSet;

use radicle::{
    node::{NodeId, Timestamp},
    prelude::RepoId,
};

pub struct SetInventory {
    /// The node we are setting the inventory for.
    pub node: NodeId,
    /// The inventory of the node.
    pub inventory: BTreeSet<RepoId>,
    /// When the inventory update was found.
    pub now: Timestamp,
}

/// An error occurred when setting the inventory for a node in the routing
/// table.
///
/// Note that there are no domain errors for setting the inventory, since we
/// expect it to always set the inventory.
pub enum SetInventoryError {
    /// An error occurred due to the underlying storage mechanism.
    Other(Box<dyn std::error::Error + Send + Sync + 'static>),
}

pub trait RoutingTable {
    /// Set the inventory for a node. The inventory is essentially the set of
    /// RIDs that the node is seeding and replicating.
    fn set_inventory(set: SetInventory) -> Result<(), SetInventoryError>;
}
