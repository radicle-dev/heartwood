use std::str::FromStr;
use std::{fmt, net};

use indexmap::IndexSet;
use localtime::LocalDuration;
use serde::{Deserialize, Serialize};
use serde_json as json;

use crate::crypto::PublicKeyError;
use crate::node::policy::SeedingPolicy;
use crate::node::{self, ParseAddressError, UserAgent};
use crate::node::{Address, Alias, NodeId};
use crate::storage::refs::FeatureLevel;

use super::policy;

/// Peer-to-peer protocol version.
pub type ProtocolVersion = u8;

/// Configured public seeds.
pub mod seeds {
    use std::{str::FromStr, sync::LazyLock};

    use crate::node::{Address, Host};

    use super::{ConnectAddress, NodeId};

    /// A helper to generate many connect addresses for a node, using port 8776.
    fn to_connect_addresses(id: NodeId, hosts: Vec<Host>) -> Vec<ConnectAddress> {
        hosts
            .into_iter()
            .map(|hostname| ConnectAddress::new(id, Address::new(hostname, 8776)))
            .collect()
    }

    /// A public Radicle seed node for the community.
    pub static RADICLE_NODE_BOOTSTRAP_IRIS: LazyLock<Vec<ConnectAddress>> = LazyLock::new(|| {
        to_connect_addresses(
            #[allow(clippy::unwrap_used)] // Value is manually verified.
            NodeId::from_str("z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7").unwrap(),
            vec![
                Host::Dns("iris.radicle.network".to_owned()),
                #[cfg(feature = "tor")]
                Host::Tor(
                    #[allow(clippy::unwrap_used)] // Value is manually verified.
                    "irisradizskwweumpydlj4oammoshkxxjur3ztcmo7cou5emc6s5lfid.onion"
                        .parse()
                        .unwrap(),
                ),
            ],
        )
    });

    /// A public Radicle seed node for the community.
    pub static RADICLE_NODE_BOOTSTRAP_ROSA: LazyLock<Vec<ConnectAddress>> = LazyLock::new(|| {
        to_connect_addresses(
            #[allow(clippy::unwrap_used)] // Value is manually verified.
            NodeId::from_str("z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo").unwrap(),
            vec![
                Host::Dns("rosa.radicle.network".to_owned()),
                #[cfg(feature = "tor")]
                Host::Tor(
                    #[allow(clippy::unwrap_used)] // Value is manually verified.
                    "rosarad5bxgdlgjnzzjygnsxrwxmoaj4vn7xinlstwglxvyt64jlnhyd.onion"
                        .parse()
                        .unwrap(),
                ),
            ],
        )
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
    pub fn bootstrap(&self) -> Vec<(Alias, ProtocolVersion, Vec<ConnectAddress>)> {
        match self {
            Self::Main => [
                (
                    "iris.radicle.network",
                    seeds::RADICLE_NODE_BOOTSTRAP_IRIS.clone(),
                ),
                (
                    "rosa.radicle.network",
                    seeds::RADICLE_NODE_BOOTSTRAP_ROSA.clone(),
                ),
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
            Self::Main => {
                let mut result = seeds::RADICLE_NODE_BOOTSTRAP_IRIS.clone();
                result.extend(seeds::RADICLE_NODE_BOOTSTRAP_ROSA.clone());
                result
            }
            Self::Test => vec![],
        }
    }
}

/// Configuration parameters defining attributes of minima and maxima.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Limits {
    /// Number of routing table entries before we start pruning.
    pub routing_max_size: LimitRoutingMaxSize,

    /// How long to keep a routing table entry before being pruned.
    pub routing_max_age: LimitRoutingMaxAge,

    /// How long to keep a gossip message entry before pruning it.
    pub gossip_max_age: LimitGossipMaxAge,

    /// Maximum number of concurrent fetches per peer connection.
    pub fetch_concurrency: LimitFetchConcurrency,

    /// Maximum number of open files.
    pub max_open_files: LimitMaxOpenFiles,

    /// Rate limiter settings.
    pub rate: RateLimits,

    /// Connection limits.
    pub connection: ConnectionLimits,

    /// Channel limits.
    pub fetch_pack_receive: FetchPackSizeLimit,

    /// How long a fetch stream is allowed to stall before being aborted.
    ///
    /// This is a per-stream inactivity timeout, not a cap on total fetch
    /// duration: if no bytes flow between the peers for longer than this, the
    /// fetch is aborted. The default value is suitable for direct TCP
    /// connections; on high-latency anonymizing transports (Tor, I2P) a larger
    /// value avoids spurious aborts during tunnel rebuilds or congestion.
    pub fetch_timeout: FetchTimeout,
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
    schemars(transparent),
    // serde's transparent and try_from/into will conflict, so we tell schemars
    // to ignore them for its generation.
    schemars(!try_from),
    schemars(!into),
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

/// The duration value provided to each fetch being performed on a node.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct FetchTimeout(LocalDuration);

impl Default for FetchTimeout {
    fn default() -> Self {
        Self(LocalDuration::from_secs(30))
    }
}

impl From<FetchTimeout> for LocalDuration {
    fn from(t: FetchTimeout) -> Self {
        t.0
    }
}

impl From<LocalDuration> for FetchTimeout {
    fn from(d: LocalDuration) -> Self {
        Self(d)
    }
}

/// Connection limits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ConnectionLimits {
    /// Max inbound connections.
    pub inbound: LimitConnectionsInbound,

    /// Max outbound connections. Note that this can be greater than the *target* number.
    pub outbound: LimitConnectionsOutbound,
}

/// Rate limits for a single connection.
///
/// Rate limiting uses a token bucket for each peer. The capacity of the bucket
/// is defined by [`RateLimit::capacity`], and refill rate of the bucket is
/// defined by [`RateLimit::fill_rate`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RateLimit {
    /// The fill rate of the token bucket, refills tokens based on the elapsed
    /// time, in seconds.
    ///
    /// For example, if the fill rate is `2`, this is equivalent to 10 tokens
    /// per 5 seconds (2 × 5 = 10).
    pub fill_rate: u64,
    /// The capacity of the token bucket, is the maximum number of tokens the bucket can hold.
    /// The bucket starts with the given capacity as the number of tokens.
    /// For each connection attempt, a token is removed from the bucket, while
    /// the fill rate adds tokens back to the bucket.
    ///
    /// For example, a capacity of 3 will allow 3 connection attempts from a
    /// given peer. If 3 attempts are made before the bucket refills, i.e. the
    /// peer attempted connections in quick succession, then the third attempt
    /// will be rejected.
    pub capacity: u64,
}

impl std::fmt::Display for RateLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RateLimit(fill_rate={}, capacity={})",
            self.fill_rate, self.capacity
        )
    }
}

/// Rate limits for inbound and outbound connections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RateLimits {
    pub inbound: LimitRateInbound,

    pub outbound: LimitRateOutbound,
}

/// Full address used to connect to a remote node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(description = "\
        A node address to connect to. Format: An Ed25519 public key in multibase encoding, \
        followed by the symbol '@', followed by an IP address, or a DNS name, or a Tor onion \
        name, or an I2P address, followed by the symbol ':', followed by a TCP port number,
        or the string 'iroh'.\
    ",
    extend("examples" = [
        "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@rosa.radicle.network:8776",
        "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@xmrhfasfg5suueegrnc4gsgyi2tyclcy5oz7f5drnrodmdtob6t2ioyd.onion:8776",
        "z6Mkvky2mnSYCTUMKRdAUoZXBXLLKtnWEkWeYQcGjjnmobAU@f2atcc7udeub5kh4nkljtjwyk7ikjviorufzgwnfwhkphljl3vhq.b32.i2p:8776",
        "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi@seed.example.com:8776",
        "z6MkkfM3tPXNPrPevKr3uSiQtHPuwnNhu2yUVjgd2jXVsVz5@192.0.2.0:31337",
        "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iroh",
    ],
    "pattern" = r"^.+@((.+:((6553[0-5])|(655[0-2][0-9])|(65[0-4][0-9]{2})|(6[0-4][0-9]{3})|([1-5][0-9]{4})|([0-5]{0,5})|([0-9]{1,4})))|iroh)$",
    ),
))]
#[serde(into = "String", try_from = "String")]
pub struct ConnectAddress {
    id: NodeId,
    addr: Address,
}

impl ConnectAddress {
    pub fn new(id: NodeId, addr: Address) -> Self {
        Self { id, addr }
    }

    pub fn iroh(id: NodeId) -> Self {
        Self {
            id,
            addr: Address::iroh(),
        }
    }

    pub fn id(&self) -> &NodeId {
        &self.id
    }

    pub fn addr(&self) -> &Address {
        &self.addr
    }

    pub fn into_pair(self) -> (NodeId, Address) {
        (self.id, self.addr)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ParseConnectAddressError {
    #[error(transparent)]
    NodeId(#[from] PublicKeyError),
    #[error(transparent)]
    Address(#[from] ParseAddressError),
    #[error("missing '@' delimiter")]
    At,
}

impl TryFrom<String> for ConnectAddress {
    type Error = ParseConnectAddressError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

impl std::str::FromStr for ConnectAddress {
    type Err = ParseConnectAddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (node, address) = s.split_once('@').ok_or(ParseConnectAddressError::At)?;

        let id = NodeId::from_str(node).map_err(ParseConnectAddressError::NodeId)?;
        let addr = Address::from_str(address).map_err(ParseConnectAddressError::Address)?;

        Ok(Self { id, addr })
    }
}

impl From<ConnectAddress> for String {
    fn from(value: ConnectAddress) -> Self {
        value.to_string()
    }
}

impl std::fmt::Display for ConnectAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.id, self.addr)
    }
}

/// Peer configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", tag = "type")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PeerConfig {
    /// Static peer set. Connect to the configured peers and maintain the connections.
    Static,
    /// Dynamic peer set.
    #[default]
    Dynamic,
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
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg(any(feature = "i2p", feature = "tor"))]
pub enum AddressConfig {
    /// Proxy connections to this address type.
    Proxy {
        /// Proxy address.
        address: net::SocketAddr,
    },
    /// Forward address to the next layer. Either this is the global proxy,
    /// or the operating system, via DNS.
    Forward,
    /// Drop connections to this address type.
    #[default]
    Drop,
}

/// Default seeding policy. Applies when no repository policies for the given repo are found.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "default", rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DefaultSeedingPolicy {
    /// Allow seeding.
    Allow {
        /// Seeding scope.
        #[serde(skip_serializing_if = "Scope::is_implicit")]
        #[cfg_attr(feature = "schemars", schemars(flatten))]
        scope: Scope,
    },
    /// Block seeding.
    #[default]
    Block,
}

/// [`Scope`] provides a schema for [`policy::Scope`], where the inner scope is
/// optional. It is introduced to allow ease migration to a future
/// version of [`DefaultSeedingPolicy::Allow`], where no or different defaults
/// apply to [`DefaultSeedingPolicy::Allow::scope`].
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Scope(Option<policy::Scope>);

impl Scope {
    /// Construct the implicit scope, where the default value,
    /// [`policy::Scope::All`], is chosen for the final scope value.
    pub fn implicit() -> Self {
        Self(None)
    }

    /// Construct the explicit scope, where the given [`policy::Scope`] is used.
    pub fn explicit(scope: policy::Scope) -> Self {
        Self(Some(scope))
    }

    /// Resolve this [`Scope`] to its [`policy::Scope`] value.
    ///
    /// If the scope is implicit, then [`policy::Scope::All`] is returned.
    pub fn into_inner(self) -> policy::Scope {
        self.0.unwrap_or(policy::Scope::All)
    }

    /// Returns `true` when the scope is implicit, i.e. no [`policy::Scope`] was
    /// given.
    pub fn is_implicit(&self) -> bool {
        self.0.is_none()
    }

    /// Construct the explicit [`Scope`] where the inner scope is
    /// [`policy::Scope::All`].
    fn all() -> Self {
        Self::explicit(policy::Scope::All)
    }

    /// Construct the explicit [`Scope`] where the inner scope is
    /// [`policy::Scope::Followed`].
    fn followed() -> Self {
        Self::explicit(policy::Scope::Followed)
    }
}

impl DefaultSeedingPolicy {
    /// Is this an "allow" policy.
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Seed everything from anyone.
    pub fn permissive() -> Self {
        Self::Allow {
            scope: Scope::all(),
        }
    }

    /// Seed only delegate changes.
    pub fn followed() -> Self {
        Self::Allow {
            scope: Scope::followed(),
        }
    }
}

impl From<DefaultSeedingPolicy> for SeedingPolicy {
    fn from(policy: DefaultSeedingPolicy) -> Self {
        match policy {
            DefaultSeedingPolicy::Block => Self::Block,
            DefaultSeedingPolicy::Allow { scope } => SeedingPolicy::Allow {
                scope: scope.into_inner(),
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct FeatureLevelConfig {
    /// The minimum feature level required to accept incoming
    /// references from other users. This value is compared
    /// against the feature level detected on refs as they are
    /// fetched.
    ///
    /// Note that by increasing this value, security can be
    /// traded for compatibility. The higher the value,
    /// the less backward compatible, but the more secure, fetches will be.
    #[serde(
        default,
        rename = "minimum",
        skip_serializing_if = "crate::serde_ext::is_default"
    )]
    min: FeatureLevel,
}

impl FeatureLevelConfig {
    pub fn min(&self) -> FeatureLevel {
        self.min
    }
}

/// Configuration for fetching repositories from
/// other nodes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Fetch {
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    signed_references: SignedReferencesConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SignedReferencesConfig {
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    feature_level: FeatureLevelConfig,
}

impl Fetch {
    pub fn feature_level_min(&self) -> FeatureLevel {
        self.signed_references.feature_level.min()
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
    /// User agent string to advertise in the node announcement, which is sent out to other nodes.
    #[serde(
        default = "crate::serde_ext::some_default::<UserAgent>",
        skip_serializing_if = "crate::serde_ext::is_some_default"
    )]
    pub user_agent: Option<UserAgent>,
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
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "std::collections::HashSet<ConnectAddress>")
    )]
    pub connect: IndexSet<ConnectAddress>,
    /// Specify the node's public addresses
    #[serde(default)]
    pub external_addresses: Vec<Address>,
    /// Global proxy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<net::SocketAddr>,
    /// Onion address config.
    #[cfg(feature = "tor")]
    #[serde(
        default,
        skip_serializing_if = "crate::serde_ext::is_default",
        deserialize_with = "crate::serde_ext::null_to_default"
    )]
    pub onion: AddressConfig,
    /// I2P address config.
    #[cfg(feature = "i2p")]
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    pub i2p: AddressConfig,
    /// Peer-to-peer network.
    #[serde(default)]
    pub network: Network,
    /// Log level.
    #[serde(default)]
    pub log: LogLevel,
    /// Whether or not our node should relay messages.
    #[serde(default, deserialize_with = "crate::serde_ext::ok_or_default")]
    pub relay: Relay,
    /// Configured service limits.
    #[serde(default)]
    pub limits: Limits,
    /// Number of worker threads to spawn.
    #[serde(default)]
    pub workers: Workers,
    /// Default seeding policy.
    #[serde(default)]
    pub seeding_policy: DefaultSeedingPolicy,
    /// Database configuration.
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    pub database: node::db::config::Config,
    /// Configuration for fetching from other nodes.
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    pub fetch: Fetch,
    /// Extra fields that aren't supported.
    #[serde(flatten, skip_serializing)]
    pub extra: json::Map<String, json::Value>,
    /// Path to a file containing an Ed25519 secret key, in OpenSSH format, i.e.
    /// with the `-----BEGIN OPENSSH PRIVATE KEY-----` header. The corresponding
    /// public key will be used as the Node ID.
    ///
    /// A decryption password cannot be configured, but passed at runtime via
    /// the environment variable `RAD_PASSPHRASE`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<std::path::PathBuf>,
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
            user_agent: Some(UserAgent::default()),
            peers: PeerConfig::default(),
            listen: vec![],
            connect: IndexSet::default(),
            external_addresses: vec![],
            network: Network::default(),
            proxy: None,
            #[cfg(feature = "tor")]
            onion: AddressConfig::Drop,
            #[cfg(feature = "i2p")]
            i2p: AddressConfig::Drop,
            relay: Relay::default(),
            limits: Limits::default(),
            workers: Workers::default(),
            log: LogLevel::default(),
            seeding_policy: DefaultSeedingPolicy::default(),
            database: node::db::config::Config::default(),
            extra: json::Map::default(),
            fetch: Fetch::default(),
            secret: None,
        }
    }

    pub fn peer(&self, id: &NodeId) -> Option<&Address> {
        self.connect
            .iter()
            .find(|ca| &ca.id == id)
            .map(|ca| &ca.addr)
    }

    pub fn peers(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.connect.iter().map(|p| p.id)
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

    /// Return the configured user agent, if set. Otherwise, fall back to the
    /// uninteresting value `"/radicle/"`.
    pub fn user_agent(&self) -> UserAgent {
        match self.user_agent.as_ref() {
            Some(agent) => agent.clone(),
            None => UserAgent::from_str("/radicle/").expect("valid user agent"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LogLevel(
    #[serde(with = "crate::serde_ext::string")]
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "crate::schemars_ext::log::Level")
    )]
    log::Level,
);

impl Default for LogLevel {
    fn default() -> Self {
        Self(log::Level::Info)
    }
}

impl From<log::Level> for LogLevel {
    fn from(value: log::Level) -> Self {
        Self(value)
    }
}

impl From<LogLevel> for log::Level {
    fn from(value: LogLevel) -> Self {
        value.0
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LimitRoutingMaxAge(localtime::LocalDuration);

impl Default for LimitRoutingMaxAge {
    fn default() -> Self {
        Self(localtime::LocalDuration::from_mins(7 * 24 * 60)) // One week
    }
}

impl From<LimitRoutingMaxAge> for LocalDuration {
    fn from(value: LimitRoutingMaxAge) -> Self {
        value.0
    }
}

impl From<LocalDuration> for LimitRoutingMaxAge {
    fn from(value: LocalDuration) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LimitGossipMaxAge(localtime::LocalDuration);

impl Default for LimitGossipMaxAge {
    fn default() -> Self {
        Self(localtime::LocalDuration::from_mins(2 * 7 * 24 * 60)) // Two weeks
    }
}

impl From<LimitGossipMaxAge> for LocalDuration {
    fn from(value: LimitGossipMaxAge) -> Self {
        value.0
    }
}

/// Create a new type (`$name`) around a given type (`$type`), with a provided
/// default (`$default`).
///
/// The macro will attempt to derive any extra `$derive`s passed.
///
/// Note that the macro will provide the following traits automatically:
///   - `Clone`
///   - `Debug`
///   - `Display`
///   - `Serialize`
///   - `Deserialize`
///   - `From<$name> for $type`, i.e. can convert back into the original type
macro_rules! wrapper {
    ($name:ident, $type:ty, $default:expr_2021 $(, $derive:ty)*) => {
        #[derive(Clone, Debug, Deserialize, Serialize $(, $derive)*)]
        #[serde(transparent)]
        #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
        pub struct $name($type);

        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }

        impl From<$type> for $name {
            fn from(value: $type) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $type {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
wrapper!(Workers, usize, 8, Copy);
wrapper!(LimitConnectionsInbound, usize, 128, Copy);
wrapper!(LimitConnectionsOutbound, usize, 16, Copy);
wrapper!(LimitRoutingMaxSize, usize, 1000, Copy);
wrapper!(LimitFetchConcurrency, usize, 1, Copy);
wrapper!(
    LimitRateInbound,
    RateLimit,
    RateLimit {
        fill_rate: 5,
        capacity: 1024,
    },
    Copy
);
wrapper!(LimitMaxOpenFiles, usize, 4096, Copy);
wrapper!(
    LimitRateOutbound,
    RateLimit,
    RateLimit {
        fill_rate: 10,
        capacity: 2048,
    },
    Copy
);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::{DefaultSeedingPolicy, Scope};
    use crate::node::{Alias, UserAgent, policy};
    use serde_json::json;

    #[test]
    fn partial() {
        use super::Config;
        use serde_json::json;

        let config: Config = serde_json::from_value(json!({
            "alias": "example",
            "limits": {
                "connection": {
                    "inbound": 1337,
                },
            },
        }
        ))
        .unwrap();
        assert_eq!(config.limits.connection.inbound.0, 1337);
        assert_eq!(
            config.limits.connection.outbound.0,
            super::LimitConnectionsOutbound::default().0,
        );

        let config: Config = serde_json::from_value(json!({
            "alias": "example",
            "limits": {
                "connection": {
                    "outbound": 1337,
                },
            },
        }
        ))
        .unwrap();
        assert_eq!(
            config.limits.connection.inbound.0,
            super::LimitConnectionsInbound::default().0,
        );
        assert_eq!(config.limits.connection.outbound.0, 1337);
    }

    #[test]
    fn deserialize_migrating_scope() {
        let seeding_policy: DefaultSeedingPolicy = serde_json::from_value(json!({
            "default": "allow"
        }))
        .unwrap();

        assert_eq!(
            seeding_policy,
            DefaultSeedingPolicy::Allow { scope: Scope(None) }
        );

        let seeding_policy: DefaultSeedingPolicy = serde_json::from_value(json!({
            "default": "allow",
            "scope": null
        }))
        .unwrap();

        assert_eq!(
            seeding_policy,
            DefaultSeedingPolicy::Allow { scope: Scope(None) }
        );

        let seeding_policy: DefaultSeedingPolicy = serde_json::from_value(json!({
            "default": "allow",
            "scope": "all"
        }))
        .unwrap();

        assert_eq!(
            seeding_policy,
            DefaultSeedingPolicy::Allow {
                scope: Scope(Some(policy::Scope::All))
            }
        );

        let seeding_policy: DefaultSeedingPolicy = serde_json::from_value(json!({
            "default": "allow",
            "scope": "followed"
        }))
        .unwrap();

        assert_eq!(
            seeding_policy,
            DefaultSeedingPolicy::Allow {
                scope: Scope(Some(policy::Scope::Followed))
            }
        )
    }

    #[test]
    fn serialize_migrating_scope() {
        assert_eq!(
            json!({
                "default": "allow"
            }),
            serde_json::to_value(DefaultSeedingPolicy::Allow { scope: Scope(None) }).unwrap()
        );

        assert_eq!(
            json!({
                "default": "allow",
                "scope": "all"
            }),
            serde_json::to_value(DefaultSeedingPolicy::Allow {
                scope: Scope(Some(policy::Scope::All))
            })
            .unwrap()
        );
        assert_eq!(
            json!({
                "default": "allow",
                "scope": "followed"
            }),
            serde_json::to_value(DefaultSeedingPolicy::Allow {
                scope: Scope(Some(policy::Scope::Followed))
            })
            .unwrap()
        );
    }

    #[test]
    fn regression_ipv6_address_brackets() {
        let address = "[2001:db8::1]:5976".to_string();
        let config = json!({
            "alias": "radicle",
            "externalAddresses": [address],
        });
        let got: super::Config = serde_json::from_value(config).unwrap();
        let mut expected = super::Config::new(Alias::new("radicle"));
        expected.external_addresses = vec![address.parse().unwrap()];
        assert_eq!(got.alias, expected.alias);
        assert_eq!(got.external_addresses, expected.external_addresses);
    }

    #[test]
    fn regression_ipv6_address_no_brackets() {
        let address = "2001:db8::1:5976".to_string();
        let config = json!({
            "alias": "radicle",
            "externalAddresses": [address],
        });
        let got: super::Config = serde_json::from_value(config).unwrap();
        let mut expected = super::Config::new(Alias::new("radicle"));
        expected.external_addresses = vec![address.parse().unwrap()];
        assert_eq!(got.alias, expected.alias);
        assert_eq!(got.external_addresses, expected.external_addresses);
    }

    #[test]
    fn iroh() {
        let config = json!({
            "alias": "radicle",
            "connect": [
                "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iroh"
            ],
        });
        let got: super::Config = serde_json::from_value(config).unwrap();
        assert_eq!(got.alias.to_string(), "radicle");
        assert_eq!(
            got.connect[0].addr().address_type(),
            crate::node::address::AddressType::Iroh
        );
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn iroh_schema() {
        use super::Config;
        use serde_json::to_value;

        let config = json!({
            "alias": "radicle",
            "connect": [
                "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iroh"
            ],
        });
        let got: Config = serde_json::from_value(config).unwrap();

        let schema = to_value(schemars::schema_for!(Config)).unwrap();
        let config = to_value(got).unwrap();
        jsonschema::validate(&schema, &config)
            .expect("generated configuration should validate under generated JSON Schema");
    }

    #[test]
    fn fetch_level_min() {
        let config = json!({
            "alias": "radicle",
            "fetch": {
                "signedReferences": {
                    "featureLevel": {
                        "minimum": "parent"
                    }
                }
            },
        });
        let got: super::Config = serde_json::from_value(config).unwrap();
        let expected = super::Config::new(Alias::new("radicle"));
        assert_eq!(got.alias, expected.alias);
        assert_eq!(
            got.fetch.feature_level_min(),
            crate::storage::refs::FeatureLevel::Parent
        );
    }

    #[cfg(feature = "tor")]
    #[test]
    fn onion_absent() {
        let actual: super::Config = serde_json::from_value(json!({
            "alias": "radicle",
        }))
        .unwrap();
        assert_eq!(super::AddressConfig::Drop, actual.onion);
    }

    #[cfg(feature = "tor")]
    #[test]
    fn onion_null() {
        // Backwards compatibility: Prior versions allowed to set `onion` to `null`,
        // which should be treated the same as the default, i.e. `Drop`.
        let actual: super::Config = serde_json::from_value(json!({
            "alias": "radicle",
            "onion": null,
        }))
        .unwrap();
        assert_eq!(super::AddressConfig::Drop, actual.onion);
    }

    #[test]
    fn user_agent_opt_out() {
        let actual: super::Config = serde_json::from_value(json!({
            "alias": "radicle",
            "userAgent": null,
        }))
        .unwrap();
        assert_eq!(None, actual.user_agent);
    }

    #[test]
    fn user_agent_default() {
        let actual: super::Config = serde_json::from_value(json!({
            "alias": "radicle",
        }))
        .unwrap();
        assert_eq!(Some(UserAgent::default()), actual.user_agent);
    }

    #[test]
    fn user_agent_custom() {
        use std::str::FromStr as _;

        let actual: super::Config = serde_json::from_value(json!({
            "alias": "radicle",
            "userAgent": "/example:0.1.0/",
        }))
        .unwrap();
        assert_eq!(
            Some(UserAgent::from_str("/example:0.1.0/").unwrap()),
            actual.user_agent
        );
    }

    #[test]
    fn user_agent_default_explicit() {
        use std::str::FromStr as _;

        let default_as_string = UserAgent::default().to_string();
        assert!(default_as_string.contains(":"));

        let actual: super::Config = serde_json::from_value(json!({
            "alias": "radicle",
            "userAgent": default_as_string,
        }))
        .unwrap();
        assert_eq!(
            Some(UserAgent::from_str(&default_as_string).unwrap()),
            actual.user_agent
        );
    }
}
