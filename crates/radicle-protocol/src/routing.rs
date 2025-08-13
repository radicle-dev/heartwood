pub mod effects;

use std::collections::BTreeSet;

use radicle::{
    node::{NodeId, Timestamp},
    prelude::RepoId,
};

/// Evaluate a node's inventory when a new inventory is announced.
pub struct Routing {
    node: NodeId,
    inventory: BTreeSet<RepoId>,
}

impl Routing {
    /// Construct the node's existing inventory.
    pub fn new(node: NodeId, inventory: BTreeSet<RepoId>) -> Self {
        Self { node, inventory }
    }

    /// When a new inventory has been found, evaluate it against the existing
    /// inventory to see which repositories were added or removed.
    pub fn found_inventory(&self, inventory: BTreeSet<RepoId>) -> RoutingResult {
        let mut result = RoutingResult::default();
        let existing = &self.inventory;
        let removed = existing
            .difference(&inventory)
            .copied()
            .collect::<BTreeSet<RepoId>>();
        let added = inventory
            .difference(existing)
            .copied()
            .collect::<BTreeSet<RepoId>>();

        result.update(inventory);
        result.added(self.node, added);
        result.removed(self.node, removed);
        result
    }

    /// Finalize the node's inventory and get the [`SetInventory`] for setting
    /// the inventory in the [`RoutingTable`].
    ///
    /// [`SetInventory`]: effects::SetInventory
    /// [`RoutingTable`]: effects::RoutingTable
    pub fn set_inventory(self, now: Timestamp) -> effects::SetInventory {
        effects::SetInventory {
            node: self.node,
            inventory: self.inventory,
            now,
        }
    }
}

impl Routing {
    /// Process a [`Command`] for the routing computation to update the state of
    /// the node's inventory.
    pub fn handle_command(&mut self, command: Command) {
        match command {
            Command::Update { inventory } => {
                self.inventory = inventory;
            }
        }
    }
}

/// Commands for advancing the [`Routing`] state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Update the [`Routing`] state with the new inventory.
    Update { inventory: BTreeSet<RepoId> },
}

/// Events that are emitted when evaluating the [`Routing`] state against new
/// input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The set of repositories were added for the node's routing table.
    Added {
        node: NodeId,
        repos: BTreeSet<RepoId>,
    },
    /// The set of repositories were removed from the node's routing table.
    Removed {
        node: NodeId,
        repos: BTreeSet<RepoId>,
    },
}

/// The result of [`Routing::found_inventory`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoutingResult {
    /// See [`Command`].
    pub commands: Vec<Command>,
    /// See [`Event`].
    pub events: Vec<Event>,
}

impl RoutingResult {
    fn update(&mut self, inventory: BTreeSet<RepoId>) {
        self.commands.push(Command::Update { inventory });
    }

    fn added(&mut self, node: NodeId, added: BTreeSet<RepoId>) {
        self.events.push(Event::Added { node, repos: added });
    }

    fn removed(&mut self, node: NodeId, removed: BTreeSet<RepoId>) {
        self.events.push(Event::Removed {
            node,
            repos: removed,
        });
    }
}
