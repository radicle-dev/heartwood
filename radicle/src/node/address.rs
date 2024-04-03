pub mod store;
pub use store::{Error, Store};

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::{hash, net};

use cyphernet::addr::HostName;
use localtime::LocalTime;
use nonempty::NonEmpty;

use crate::collections::RandomMap;
use crate::node::{Address, Alias, Penalty, UserAgent};
use crate::prelude::Timestamp;
use crate::{node, profile};

/// A map with the ability to randomly select values.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AddressBook<K: hash::Hash + Eq, V> {
    inner: RandomMap<K, V>,
    #[serde(skip)]
    rng: RefCell<fastrand::Rng>,
}

impl<K: hash::Hash + Eq, V> AddressBook<K, V> {
    /// Create a new address book.
    pub fn new(rng: fastrand::Rng) -> Self {
        Self {
            inner: RandomMap::with_hasher(rng.clone().into()),
            rng: RefCell::new(rng),
        }
    }

    /// Pick a random value in the book.
    pub fn sample(&self) -> Option<(&K, &V)> {
        self.sample_with(|_, _| true)
    }

    /// Pick a random value in the book matching a predicate.
    pub fn sample_with(&self, mut predicate: impl FnMut(&K, &V) -> bool) -> Option<(&K, &V)> {
        if let Some(pairs) = NonEmpty::from_vec(
            self.inner
                .iter()
                .filter(|(k, v)| predicate(*k, *v))
                .collect(),
        ) {
            let ix = self.rng.borrow_mut().usize(..pairs.len());
            let pair = pairs[ix]; // Can't fail.

            Some(pair)
        } else {
            None
        }
    }

    /// Return a new address book with the given RNG.
    pub fn with(self, rng: fastrand::Rng) -> Self {
        Self {
            inner: self.inner,
            rng: RefCell::new(rng),
        }
    }
}

impl<K: hash::Hash + Eq + Ord + Copy, V> AddressBook<K, V> {
    /// Return a shuffled iterator.
    pub fn shuffled(&self) -> std::vec::IntoIter<(&K, &V)> {
        let mut items = self.inner.iter().collect::<Vec<_>>();
        items.sort_by_key(|(k, _)| *k);
        self.rng.borrow_mut().shuffle(&mut items);

        items.into_iter()
    }

    /// Turn this object into a shuffled iterator.
    pub fn into_shuffled(self) -> impl Iterator<Item = (K, V)> {
        let mut items = self.inner.into_iter().collect::<Vec<_>>();
        items.sort_by_key(|(k, _)| *k);
        self.rng.borrow_mut().shuffle(&mut items);

        items.into_iter()
    }

    /// Cycle through the keys at random. The random cycle repeats ad-infintum.
    pub fn cycle(&self) -> impl Iterator<Item = &K> {
        self.shuffled().map(|(k, _)| k).cycle()
    }
}

impl<K: hash::Hash + Eq, V> FromIterator<(K, V)> for AddressBook<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let rng = profile::env::rng();
        let mut inner = RandomMap::with_hasher(rng.clone().into());

        for (k, v) in iter {
            inner.insert(k, v);
        }
        Self {
            inner,
            rng: RefCell::new(rng),
        }
    }
}

impl<K: hash::Hash + Eq, V> Deref for AddressBook<K, V> {
    type Target = RandomMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<K: hash::Hash + Eq, V> DerefMut for AddressBook<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Node public data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Protocol version.
    pub version: u8,
    /// Advertized alias.
    pub alias: Alias,
    /// Advertized features.
    pub features: node::Features,
    /// Advertized addresses
    pub addrs: Vec<KnownAddress>,
    /// Proof-of-work included in node announcement.
    pub pow: u32,
    /// When this data was published.
    pub timestamp: Timestamp,
    /// User agent string.
    pub agent: UserAgent,
    /// Node connection penalty.
    pub penalty: Penalty,
    /// Whether the node is banned.
    pub banned: bool,
}

/// A known address.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownAddress {
    /// Network address.
    pub addr: Address,
    /// Address of the peer who sent us this address.
    pub source: Source,
    /// Last time this address was used to successfully connect to a peer.
    #[serde(with = "crate::serde_ext::localtime::option::time")]
    pub last_success: Option<LocalTime>,
    /// Last time this address was tried.
    #[serde(with = "crate::serde_ext::localtime::option::time")]
    pub last_attempt: Option<LocalTime>,
    /// Whether this address has been banned.
    pub banned: bool,
}

impl KnownAddress {
    /// Create a new known address.
    pub fn new(addr: Address, source: Source) -> Self {
        Self {
            addr,
            source,
            last_success: None,
            last_attempt: None,
            banned: false,
        }
    }
}

/// Address source. Specifies where an address originated from.
#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// An address that was shared by another peer.
    Peer,
    /// A bootstrap node address.
    Bootstrap,
    /// An address that came from some source external to the system, eg.
    /// specified by the user or added directly to the address manager.
    Imported,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer => write!(f, "Peer"),
            Self::Bootstrap => write!(f, "Bootstrap"),
            Self::Imported => write!(f, "Imported"),
        }
    }
}

/// Address type.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    Ipv4 = 1,
    Ipv6 = 2,
    Dns = 3,
    Onion = 4,
}

impl From<AddressType> for u8 {
    fn from(other: AddressType) -> Self {
        other as u8
    }
}

impl From<&Address> for AddressType {
    fn from(a: &Address) -> Self {
        match a.host {
            HostName::Ip(net::IpAddr::V4(_)) => AddressType::Ipv4,
            HostName::Ip(net::IpAddr::V6(_)) => AddressType::Ipv6,
            HostName::Dns(_) => AddressType::Dns,
            HostName::Tor(_) => AddressType::Onion,
            _ => todo!(), // FIXME(cloudhead): Maxim will remove `non-exhaustive`
        }
    }
}

impl TryFrom<u8> for AddressType {
    type Error = u8;

    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(AddressType::Ipv4),
            2 => Ok(AddressType::Ipv6),
            3 => Ok(AddressType::Dns),
            4 => Ok(AddressType::Onion),
            _ => Err(other),
        }
    }
}
/// Check whether an IP address is globally routable.
pub fn is_routable(addr: &net::IpAddr) -> bool {
    match addr {
        net::IpAddr::V4(addr) => ipv4_is_routable(addr),
        net::IpAddr::V6(addr) => ipv6_is_routable(addr),
    }
}

/// Check whether an IP address is locally routable.
pub fn is_local(addr: &net::IpAddr) -> bool {
    match addr {
        net::IpAddr::V4(addr) => {
            addr.is_private() || addr.is_loopback() || addr.is_link_local() || addr.is_unspecified()
        }
        net::IpAddr::V6(_) => false,
    }
}

/// Check whether an IPv4 address is globally routable.
///
/// This code is adapted from the Rust standard library's `net::Ipv4Addr::is_global`. It can be
/// replaced once that function is stabilized.
fn ipv4_is_routable(addr: &net::Ipv4Addr) -> bool {
    // Check if this address is 192.0.0.9 or 192.0.0.10. These addresses are the only two
    // globally routable addresses in the 192.0.0.0/24 range.
    if u32::from(*addr) == 0xc0000009 || u32::from(*addr) == 0xc000000a {
        return true;
    }
    !addr.is_private()
        && !addr.is_loopback()
        && !addr.is_link_local()
        && !addr.is_broadcast()
        && !addr.is_documentation()
        // Make sure the address is not in 0.0.0.0/8.
        && addr.octets()[0] != 0
}

/// Check whether an IPv6 address is globally routable.
///
/// For now, this always returns `true`, as IPv6 addresses
/// are not fully supported.
fn ipv6_is_routable(_addr: &net::Ipv6Addr) -> bool {
    true
}
