use localtime::LocalDuration;

use radicle::node::Address;

use crate::service::tracking::Policy;
use crate::service::NodeId;

/// Peer-to-peer network.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Network {
    #[default]
    Main,
    Test,
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
    pub connect: Vec<(NodeId, Address)>,
    /// Specify the node's public addresses
    pub external_addresses: Vec<Address>,
    /// Peer-to-peer network.
    pub network: Network,
    /// Whether or not our node should relay inventories.
    pub relay: bool,
    /// Configured service limits.
    pub limits: Limits,
    /// Default tracking policy.
    pub policy: Policy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect: Vec::default(),
            external_addresses: vec![],
            network: Network::default(),
            relay: true,
            limits: Limits::default(),
            policy: Policy::Block,
        }
    }
}

impl Config {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            ..Self::default()
        }
    }

    pub fn peer(&self, id: &NodeId) -> Option<&Address> {
        self.connect.iter().find(|(i, _)| i == id).map(|(_, a)| a)
    }

    pub fn is_persistent(&self, id: &NodeId) -> bool {
        self.connect.iter().any(|(i, _)| i == id)
    }

    pub fn alias(&self) -> [u8; 32] {
        let mut alias = [0u8; 32];

        alias[..9].copy_from_slice("anonymous".as_bytes());
        alias
    }
}
