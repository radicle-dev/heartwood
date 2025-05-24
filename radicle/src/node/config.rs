use std::collections::HashSet;
use std::ops::Deref;
use std::str::FromStr;
use std::{fmt, net};

use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;
use serde::{Deserialize, Serialize};
use serde_json as json;

use crate::node;
use crate::node::policy::{Scope, SeedingPolicy};
use crate::node::{Address, Alias, NodeId};

/// Peer-to-peer protocol version.
pub type ProtocolVersion = u8;

/// Default number of workers to spawn.
pub const DEFAULT_WORKERS: usize = 8;

/// Configured public seeds.
pub mod seeds {
    use std::str::FromStr;

    use super::{ConnectAddress, PeerAddr};
    use once_cell::sync::Lazy;

    /// The radicle public community seed node.
    pub static RADICLE_COMMUNITY_NODE: Lazy<ConnectAddress> = Lazy::new(|| {
        // SAFETY: `ConnectAddress` is known at compile time.
        #[allow(clippy::unwrap_used)]
        PeerAddr::from_str(
            "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776",
        )
        .unwrap()
        .into()
    });

    /// The radicle public `ash` seed node.
    pub static RADICLE_ASH_NODE: Lazy<ConnectAddress> = Lazy::new(|| {
        // SAFETY: `ConnectAddress` is known at compile time.
        #[allow(clippy::unwrap_used)]
        PeerAddr::from_str(
            "z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@ash.radicle.garden:8776",
        )
        .unwrap()
        .into()
    });

    /// The radicle team node.
    pub static RADICLE_TEAM_NODE: Lazy<ConnectAddress> = Lazy::new(|| {
        // SAFETY: `ConnectAddress` is known at compile time.
        #[allow(clippy::unwrap_used)]
        PeerAddr::from_str("z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz@seed.radicle.xyz:8776")
            .unwrap()
            .into()
    });
}

/// Peer-to-peer network.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Network {
    #[default]
    Main,
    Test,
}

impl Network {
    /// Bootstrap nodes for this network.
    pub fn bootstrap(&self) -> Vec<(Alias, ProtocolVersion, ConnectAddress)> {
        match self {
            Self::Main => [
                ("seed.radicle.garden", seeds::RADICLE_COMMUNITY_NODE.clone()),
                ("seed.radicle.xyz", seeds::RADICLE_TEAM_NODE.clone()),
            ]
            .into_iter()
            .map(|(a, s)| (Alias::new(a), 1, s))
            .collect(),

            Self::Test => vec![],
        }
    }

    /// Public seeds for this network.
    pub fn public_seeds(&self) -> Vec<ConnectAddress> {
        match self {
            Self::Main => vec![
                seeds::RADICLE_COMMUNITY_NODE.clone(),
                seeds::RADICLE_ASH_NODE.clone(),
            ],
            Self::Test => vec![],
        }
    }
}

/// Configuration parameters defining attributes of minima and maxima.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Limits {
    /// Number of routing table entries before we start pruning.
    pub routing_max_size: usize,
    /// How long to keep a routing table entry before being pruned.
    #[serde(with = "crate::serde_ext::localtime::duration")]
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "crate::schemars_ext::localtime::LocalDuration")
    )]
    pub routing_max_age: LocalDuration,
    /// How long to keep a gossip message entry before pruning it.
    #[serde(with = "crate::serde_ext::localtime::duration")]
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "crate::schemars_ext::localtime::LocalDuration")
    )]
    pub gossip_max_age: LocalDuration,
    /// Maximum number of concurrent fetches per peer connection.
    pub fetch_concurrency: usize,
    /// Maximum number of open files.
    pub max_open_files: usize,
    /// Rate limitter settings.
    #[serde(default)]
    pub rate: RateLimits,
    /// Connection limits.
    #[serde(default)]
    pub connection: ConnectionLimits,
    /// Channel limits.
    #[serde(default)]
    pub fetch_pack_receive: FetchPackSizeLimit,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            routing_max_size: 1000,
            routing_max_age: LocalDuration::from_mins(7 * 24 * 60), // One week
            gossip_max_age: LocalDuration::from_mins(2 * 7 * 24 * 60), // Two weeks
            fetch_concurrency: 1,
            max_open_files: 4096,
            rate: RateLimits::default(),
            connection: ConnectionLimits::default(),
            fetch_pack_receive: FetchPackSizeLimit::default(),
        }
    }
}

/// Limiter for byte streams.
///
/// Default: 500MiB
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(into = "String", try_from = "String")]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(transparent)
)]
pub struct FetchPackSizeLimit {
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "crate::schemars_ext::bytesize::ByteSize")
    )]
    limit: bytesize::ByteSize,
}

impl From<bytesize::ByteSize> for FetchPackSizeLimit {
    fn from(limit: bytesize::ByteSize) -> Self {
        Self { limit }
    }
}

impl From<FetchPackSizeLimit> for String {
    fn from(limit: FetchPackSizeLimit) -> Self {
        limit.to_string()
    }
}

impl TryFrom<String> for FetchPackSizeLimit {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl FromStr for FetchPackSizeLimit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(FetchPackSizeLimit { limit: s.parse()? })
    }
}

impl fmt::Display for FetchPackSizeLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.limit)
    }
}

impl FetchPackSizeLimit {
    /// New `FetchPackSizeLimit` in bytes.
    pub fn bytes(size: u64) -> Self {
        bytesize::ByteSize::b(size).into()
    }

    /// New `FetchPackSizeLimit` in kibibytes.
    pub fn kibibytes(size: u64) -> Self {
        bytesize::ByteSize::kib(size).into()
    }

    /// New `FetchPackSizeLimit` in mebibytes.
    pub fn mebibytes(size: u64) -> Self {
        bytesize::ByteSize::mib(size).into()
    }

    /// New `FetchPackSizeLimit` in gibibytes.
    pub fn gibibytes(size: u64) -> Self {
        bytesize::ByteSize::gib(size).into()
    }

    /// Check if this limit is exceeded by the number of `bytes` provided.
    pub fn exceeded_by(&self, bytes: usize) -> bool {
        bytes >= self.limit.as_u64() as usize
    }
}

impl Default for FetchPackSizeLimit {
    fn default() -> Self {
        Self::mebibytes(500)
    }
}

/// Connection limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ConnectionLimits {
    /// Max inbound connections.
    pub inbound: usize,
    /// Max outbound connections. Note that this can be higher than the *target* number.
    pub outbound: usize,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            inbound: 128,
            outbound: 16,
        }
    }
}

/// Rate limts for a single connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RateLimit {
    pub fill_rate: f64,
    pub capacity: usize,
}

/// Rate limits for inbound and outbound connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RateLimits {
    pub inbound: RateLimit,
    pub outbound: RateLimit,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            inbound: RateLimit {
                fill_rate: 5.0,
                capacity: 1024,
            },
            outbound: RateLimit {
                fill_rate: 10.0,
                capacity: 2048,
            },
        }
    }
}

/// Full address used to connect to a remote node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(description = "\
    A node address to connect to. Format: An Ed25519 public key in multibase encoding, \
    followed by the symbol '@', followed by an IP address, or a DNS name, or a Tor onion \
    name, followed by the symbol ':', followed by a TCP port number.\
")
)]
pub struct ConnectAddress(
    #[serde(with = "crate::serde_ext::string")]
    #[cfg_attr(feature = "schemars", schemars(
        with = "String",
        regex(pattern = r"^.+@.+:((6553[0-5])|(655[0-2][0-9])|(65[0-4][0-9]{2})|(6[0-4][0-9]{3})|([1-5][0-9]{4})|([0-5]{0,5})|([0-9]{1,4}))$"),
        extend("examples" = [
            "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776",
            "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@xmrhfasfg5suueegrnc4gsgyi2tyclcy5oz7f5drnrodmdtob6t2ioyd.onion:8776",
            "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi@seed.example.com:8776",
            "z6MkkfM3tPXNPrPevKr3uSiQtHPuwnNhu2yUVjgd2jXVsVz5@192.0.2.0:31337",
        ]),
    ))]
    PeerAddr<NodeId, Address>,
);

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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PeerConfig {
    /// Static peer set. Connect to the configured peers and maintain the connections.
    Static,
    /// Dynamic peer set.
    Dynamic,
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self::Dynamic
    }
}

/// Relay configuration.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Relay {
    /// Always relay messages.
    Always,
    /// Never relay messages.
    Never,
    /// Relay messages when applicable.
    #[default]
    Auto,
}

/// Proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AddressConfig {
    /// Proxy connections to this address type.
    Proxy {
        /// Proxy address.
        address: net::SocketAddr,
    },
    /// Forward address to the next layer. Either this is the global proxy,
    /// or the operating system, via DNS.
    Forward,
}

/// Default seeding policy. Applies when no repository policies for the given repo are found.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "default")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DefaultSeedingPolicy {
    /// Allow seeding.
    Allow {
        /// Seeding scope.
        #[serde(default)]
        scope: Scope,
    },
    /// Block seeding.
    #[default]
    Block,
}

impl DefaultSeedingPolicy {
    /// Is this an "allow" policy.
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Seed everything from anyone.
    pub fn permissive() -> Self {
        Self::Allow { scope: Scope::All }
    }
}

impl From<DefaultSeedingPolicy> for SeedingPolicy {
    fn from(policy: DefaultSeedingPolicy) -> Self {
        match policy {
            DefaultSeedingPolicy::Block => Self::Block,
            DefaultSeedingPolicy::Allow { scope } => Self::Allow { scope },
        }
    }
}

/// Service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(rename = "NodeConfig")
)]
pub struct Config {
    /// Node alias.
    pub alias: Alias,
    /// Socket address (a combination of IPv4 or IPv6 address and TCP port) to listen on.
    #[serde(default)]
    #[cfg_attr(feature = "schemars", schemars(example = &"127.0.0.1:8776"))]
    pub listen: Vec<net::SocketAddr>,
    /// Peer configuration.
    #[serde(default)]
    pub peers: PeerConfig,
    /// Peers to connect to on startup.
    /// Connections to these peers will be maintained.
    #[serde(default)]
    pub connect: HashSet<ConnectAddress>,
    /// Specify the node's public addresses
    #[serde(default)]
    pub external_addresses: Vec<Address>,
    /// Global proxy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<net::SocketAddr>,
    /// Onion address config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onion: Option<AddressConfig>,
    /// Peer-to-peer network.
    #[serde(default)]
    pub network: Network,
    /// Log level.
    #[serde(default = "defaults::log")]
    #[serde(with = "crate::serde_ext::string")]
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "crate::schemars_ext::log::Level")
    )]
    pub log: log::Level,
    /// Whether or not our node should relay messages.
    #[serde(default, deserialize_with = "crate::serde_ext::ok_or_default")]
    pub relay: Relay,
    /// Configured service limits.
    #[serde(default)]
    pub limits: Limits,
    /// Number of worker threads to spawn.
    #[serde(default = "defaults::workers")]
    pub workers: usize,
    /// Default seeding policy.
    #[serde(default)]
    pub seeding_policy: DefaultSeedingPolicy,
    /// Extra fields that aren't supported.
    #[serde(flatten, skip_serializing)]
    pub extra: json::Map<String, json::Value>,
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
            listen: vec![],
            connect: HashSet::default(),
            external_addresses: vec![],
            network: Network::default(),
            proxy: None,
            onion: None,
            relay: Relay::default(),
            limits: Limits::default(),
            workers: DEFAULT_WORKERS,
            log: defaults::log(),
            seeding_policy: DefaultSeedingPolicy::default(),
            extra: json::Map::default(),
        }
    }

    pub fn peer(&self, id: &NodeId) -> Option<&Address> {
        self.connect
            .iter()
            .find(|ca| &ca.id == id)
            .map(|ca| &ca.addr)
    }

    pub fn peers(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.connect.iter().cloned().map(|p| p.id)
    }

    pub fn is_persistent(&self, id: &NodeId) -> bool {
        self.peer(id).is_some()
    }

    /// Are we a relay node? This determines what we do with gossip messages from other peers.
    pub fn is_relay(&self) -> bool {
        match self.relay {
            // In "auto" mode, we only relay if we're a public seed node.
            // This reduces traffic for private nodes, as well as message redundancy.
            Relay::Auto => !self.external_addresses.is_empty(),
            Relay::Never => false,
            Relay::Always => true,
        }
    }

    pub fn features(&self) -> node::Features {
        node::Features::SEED
    }
}

/// Defaults as functions, for serde.
mod defaults {
    /// Worker count.
    pub fn workers() -> usize {
        super::DEFAULT_WORKERS
    }

    /// Log level.
    pub fn log() -> log::Level {
        log::Level::Info
    }
}
