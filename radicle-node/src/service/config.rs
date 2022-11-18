use super::nakamoto::LocalDuration;

use crate::collections::HashSet;
use crate::identity::{Id, PublicKey};
use crate::service::filter::Filter;
use crate::service::message::Address;

/// Peer-to-peer network.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Network {
    #[default]
    Main,
    Test,
}

/// Project tracking policy.
#[derive(Debug, Clone)]
pub enum ProjectTracking {
    /// Track all projects we come across.
    All { blocked: HashSet<Id> },
    /// Track a static list of projects.
    Allowed(HashSet<Id>),
}

impl Default for ProjectTracking {
    fn default() -> Self {
        Self::All {
            blocked: HashSet::default(),
        }
    }
}

/// Project remote tracking policy.
#[derive(Debug, Default, Clone)]
pub enum RemoteTracking {
    /// Only track remotes of project delegates.
    #[default]
    DelegatesOnly,
    /// Track all remotes.
    All { blocked: HashSet<PublicKey> },
    /// Track a specific list of users as well as the project delegates.
    Allowed(HashSet<PublicKey>),
}

/// Configuration parameters defining attributes of minima and maxima.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Number of routing table entries before we start pruning.
    pub routing_max_size: usize,
    /// How long to keep a routing table entry before being pruned.
    pub routing_max_age: LocalDuration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            routing_max_size: 1000,
            routing_max_age: LocalDuration::from_mins(7 * 24 * 60),
        }
    }
}

/// Service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Peers to connect to on startup.
    /// Connections to these peers will be maintained.
    pub connect: Vec<Address>,
    /// Specify the node's public addresses
    pub external_addresses: Vec<Address>,
    /// Peer-to-peer network.
    pub network: Network,
    /// Project tracking policy.
    pub project_tracking: ProjectTracking,
    /// Project remote tracking policy.
    pub remote_tracking: RemoteTracking,
    /// Whether or not our node should relay inventories.
    pub relay: bool,
    /// List of addresses to listen on for protocol connections.
    pub listen: Vec<Address>,
    pub limits: Limits,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect: Vec::default(),
            external_addresses: Vec::default(),
            network: Network::default(),
            project_tracking: ProjectTracking::default(),
            remote_tracking: RemoteTracking::default(),
            relay: true,
            listen: vec![],
            limits: Limits::default(),
        }
    }
}

impl Config {
    pub fn is_persistent(&self, addr: &Address) -> bool {
        self.connect.contains(addr)
    }

    pub fn is_tracking(&self, id: &Id) -> bool {
        match &self.project_tracking {
            ProjectTracking::All { blocked } => !blocked.contains(id),
            ProjectTracking::Allowed(ids) => ids.contains(id),
        }
    }

    /// Track a project. Returns whether the policy was updated.
    pub fn track(&mut self, id: Id) -> bool {
        match &mut self.project_tracking {
            ProjectTracking::All { .. } => false,
            ProjectTracking::Allowed(ids) => ids.insert(id),
        }
    }

    /// Untrack a project. Returns whether the policy was updated.
    pub fn untrack(&mut self, id: Id) -> bool {
        match &mut self.project_tracking {
            ProjectTracking::All { blocked } => blocked.insert(id),
            ProjectTracking::Allowed(ids) => ids.remove(&id),
        }
    }

    pub fn filter(&self) -> Filter {
        match &self.project_tracking {
            ProjectTracking::All { .. } => Filter::default(),
            ProjectTracking::Allowed(ids) => Filter::new(ids.iter()),
        }
    }

    pub fn alias(&self) -> [u8; 32] {
        let mut alias = [0u8; 32];

        alias[..9].copy_from_slice("anonymous".as_bytes());
        alias
    }
}
