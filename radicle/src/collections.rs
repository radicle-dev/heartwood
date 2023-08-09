//! Useful collections for peer-to-peer networking.
use siphasher::sip::SipHasher13;

/// A `HashMap` which uses [`fastrand::Rng`] for its random state.
pub type RandomMap<K, V> = std::collections::HashMap<K, V, RandomState>;

/// A `HashSet` which uses [`fastrand::Rng`] for its random state.
pub type RandomSet<K> = std::collections::HashSet<K, RandomState>;

/// Random hasher state.
#[derive(Clone)]
pub struct RandomState {
    key1: u64,
    key2: u64,
}

impl Default for RandomState {
    fn default() -> Self {
        Self::new(crate::profile::env::rng())
    }
}

impl RandomState {
    fn new(rng: fastrand::Rng) -> Self {
        Self {
            key1: rng.u64(..),
            key2: rng.u64(..),
        }
    }
}

impl std::hash::BuildHasher for RandomState {
    type Hasher = SipHasher13;

    fn build_hasher(&self) -> Self::Hasher {
        SipHasher13::new_with_keys(self.key1, self.key2)
    }
}

impl From<fastrand::Rng> for RandomState {
    fn from(rng: fastrand::Rng) -> Self {
        Self::new(rng)
    }
}
