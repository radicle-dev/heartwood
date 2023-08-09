use std::collections::HashSet;
use std::ops::Deref;

use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;

use crate::node;
use crate::node::tracking::{Policy, Scope};
use crate::node::{Address, Alias, NodeId};

/// Target number of peers to maintain connections to.
pub const TARGET_OUTBOUND_PEERS: usize = 8;

/// Peer-to-peer network.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Network {
    #[default]
    Main,
    Test,
}

impl Network {
    /// Bootstrap nodes for this network.
    pub fn bootstrap(&self) -> Vec<(Alias, ConnectAddress)> {
        use std::str::FromStr;

        match self {
            Self::Main => [
                (
                    "seed.radicle.garden",
                    "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776",
                ),
                (
                    "seed.radicle.xyz",
                    "z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz@seed.radicle.xyz:8776",
                ),
            ]
            .into_iter()
            // SAFETY: These are valid addresses.
            .map(|(a, s)| (Alias::new(a), PeerAddr::from_str(s).unwrap().into()))
            .collect(),

            Self::Test => vec![],
        }
    }
}

/// Configuration parameters defining attributes of minima and maxima.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// Number of routing table entries before we start pruning.
    pub routing_max_size: usize,
    /// How long to keep a routing table entry before being pruned.
    #[serde(with = "crate::serde_ext::localtime::duration")]
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

/// Full address used to connect to a remote node.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
#[serde(transparent)]
pub struct ConnectAddress(#[serde(with = "crate::serde_ext::string")] PeerAddr<NodeId, Address>);

impl From<PeerAddr<NodeId, Address>> for ConnectAddress {
    fn from(value: PeerAddr<NodeId, Address>) -> Self {
        Self(value)
    }
}

impl From<ConnectAddress> for (NodeId, Address) {
    fn from(value: ConnectAddress) -> Self {
        (value.0.id, value.0.addr)
    }
}

impl From<(NodeId, Address)> for ConnectAddress {
    fn from((id, addr): (NodeId, Address)) -> Self {
        Self(PeerAddr { id, addr })
    }
}

impl From<ConnectAddress> for Address {
    fn from(value: ConnectAddress) -> Self {
        value.0.addr
    }
}

impl Deref for ConnectAddress {
    type Target = PeerAddr<NodeId, Address>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Peer configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PeerConfig {
    /// Static peer set. Connect to the configured peers and maintain the connections.
    Static,
    /// Dynamic peer set.
    Dynamic { target: usize },
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self::Dynamic {
            target: TARGET_OUTBOUND_PEERS,
        }
    }
}

/// Service configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Node alias.
    pub alias: Alias,
    /// Peer configuration.
    pub peers: PeerConfig,
    /// Peers to connect to on startup.
    /// Connections to these peers will be maintained.
    pub connect: HashSet<ConnectAddress>,
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

impl Config {
    pub fn test(alias: Alias) -> Self {
        Self {
            network: Network::Test,
            ..Self::new(alias)
        }
    }

    pub fn new(alias: Alias) -> Self {
        Self {
            alias,
            peers: PeerConfig::default(),
            connect: HashSet::default(),
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
    pub fn peer(&self, id: &NodeId) -> Option<&Address> {
        self.connect
            .iter()
            .find(|ca| &ca.id == id)
            .map(|ca| &ca.addr)
    }

    pub fn is_persistent(&self, id: &NodeId) -> bool {
        self.peer(id).is_some()
    }

    pub fn features(&self) -> node::Features {
        node::Features::SEED
    }
}
