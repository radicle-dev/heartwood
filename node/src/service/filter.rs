use std::ops::{Deref, DerefMut};

pub use bloomy::BloomFilter;

use crate::identity::Id;

/// Size in bytes of subscription bloom filter.
pub const FILTER_SIZE: usize = 1024 * 16;
/// Number of hashes used for bloom filter.
pub const FILTER_HASHES: usize = 7;

/// Subscription filter.
///
/// The [`Default`] instance has all bits set to `1`, ie. it will match
/// everything.
///
/// Nb. This filter doesn't currently support inserting public keys.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Filter(BloomFilter<Id>);

impl Default for Filter {
    fn default() -> Self {
        Self(BloomFilter::from(vec![0xff; FILTER_SIZE]))
    }
}

impl Filter {
    pub fn new<'a>(ids: impl IntoIterator<Item = &'a Id>) -> Self {
        let mut bloom = BloomFilter::with_size(FILTER_SIZE);

        for id in ids.into_iter() {
            bloom.insert(id);
        }
        Self(bloom)
    }
}

impl Deref for Filter {
    type Target = BloomFilter<Id>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Filter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<BloomFilter<Id>> for Filter {
    fn from(bloom: BloomFilter<Id>) -> Self {
        Self(bloom)
    }
}
