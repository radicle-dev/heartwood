use std::ops::{Deref, DerefMut};

use localtime::LocalTime;
use nonempty::NonEmpty;

use crate::collections::HashMap;
use crate::node;
use crate::node::Address;
use crate::prelude::Timestamp;

/// A map with the ability to randomly select values.
#[derive(Debug, Clone)]
pub struct AddressBook<K, V> {
    inner: HashMap<K, V>,
    rng: fastrand::Rng,
}

impl<K, V> AddressBook<K, V> {
    /// Create a new address book.
    pub fn new(rng: fastrand::Rng) -> Self {
        Self {
            inner: HashMap::with_hasher(rng.clone().into()),
            rng,
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
            let ix = self.rng.usize(..pairs.len());
            let pair = pairs[ix]; // Can't fail.

            Some(pair)
        } else {
            None
        }
    }

    /// Cycle through the keys at random. The random cycle repeats ad-infintum.
    pub fn cycle(&self) -> impl Iterator<Item = &K> {
        self.shuffled().map(|(k, _)| k).cycle()
    }

    /// Return a shuffled iterator over the keys.
    pub fn shuffled(&self) -> std::vec::IntoIter<(&K, &V)> {
        let mut keys = self.inner.iter().collect::<Vec<_>>();
        self.rng.shuffle(&mut keys);

        keys.into_iter()
    }
}

impl<K, V> Deref for AddressBook<K, V> {
    type Target = HashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<K, V> DerefMut for AddressBook<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Node public data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Advertized alias.
    pub alias: Option<String>,
    /// Advertized features.
    pub features: node::Features,
    /// Advertized addresses
    pub addrs: Vec<KnownAddress>,
    /// Proof-of-work included in node announcement.
    pub pow: u32,
    /// When this data was published.
    pub timestamp: Timestamp,
}

/// A known address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownAddress {
    /// Network address.
    pub addr: Address,
    /// Address of the peer who sent us this address.
    pub source: Source,
    /// Last time this address was used to successfully connect to a peer.
    pub last_success: Option<LocalTime>,
    /// Last time this address was tried.
    pub last_attempt: Option<LocalTime>,
}

impl KnownAddress {
    /// Create a new known address.
    pub fn new(addr: Address, source: Source) -> Self {
        Self {
            addr,
            source,
            last_success: None,
            last_attempt: None,
        }
    }
}

/// Address source. Specifies where an address originated from.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Source {
    /// An address that was shared by another peer.
    Peer,
    /// An address that came from a DNS seed.
    Dns,
    /// An address that came from some source external to the system, eg.
    /// specified by the user or added directly to the address manager.
    Imported,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer => write!(f, "Peer"),
            Self::Dns => write!(f, "DNS"),
            Self::Imported => write!(f, "Imported"),
        }
    }
}
