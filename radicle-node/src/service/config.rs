use localtime::LocalDuration;

use radicle::node;
use radicle::node::Address;

use crate::bounded::BoundedVec;
use crate::service::message::{NodeAnnouncement, ADDRESS_LIMIT};
use crate::service::tracking::{Policy, Scope};
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
    /// Maximum number of concurrent fetches per per connection.
    pub fetch_concurrency: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            routing_max_size: 1000,
            routing_max_age: LocalDuration::from_mins(7 * 24 * 60),
            fetch_concurrency: 1,
        }
    }
}

/// Service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Alias chosen by the operator.
    /// Doesn't have to be unique on the network.
    pub alias: Option<String>,
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
    /// Default tracking scope.
    pub scope: Scope,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            alias: None,
            connect: Vec::default(),
            external_addresses: vec![],
            network: Network::default(),
            relay: true,
            limits: Limits::default(),
            policy: Policy::default(),
            scope: Scope::default(),
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

    pub fn features(&self) -> node::Features {
        node::Features::SEED
    }

    /// Check if a node announcement matches this configuration.
    pub fn matches(&self, other: &NodeAnnouncement) -> bool {
        let ann = self.node(other.timestamp);

        ann.features == other.features
            && ann.alias == other.alias
            && ann.addresses == other.addresses
    }

    pub fn alias(&self) -> [u8; 32] {
        let mut alias = [0u8; 32];

        if let Some(name) = &self.alias {
            alias[..name.len()].copy_from_slice(name.as_bytes());
        }
        alias
    }

    pub fn node(&self, timestamp: node::Timestamp) -> NodeAnnouncement {
        let features = self.features();
        let alias = self.alias();
        let addresses: BoundedVec<_, ADDRESS_LIMIT> = self
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_announcement() {
        let cfg = Config {
            alias: Some(String::from("cloudhead")),
            ..Config::default()
        };
        assert_eq!("cloudhead", cfg.node(0).alias().unwrap());
    }
}
