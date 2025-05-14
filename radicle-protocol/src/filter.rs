#![allow(clippy::identity_op)]
use std::ops::{Deref, DerefMut};

pub use bloomy::BloomFilter;

use radicle::identity::RepoId;

/// Size in bytes of *large* bloom filter.
/// It can store about 13'675 items with a false positive rate of 1%.
pub const FILTER_SIZE_L: usize = 16 * 1024;
/// Size in bytes of *medium* bloom filter.
/// It can store about 3'419 items with a false positive rate of 1%.
pub const FILTER_SIZE_M: usize = 4 * 1024;
/// Size in bytes of *small* bloom filter.
/// It can store about 855 items with a false positive rate of 1%.
pub const FILTER_SIZE_S: usize = 1 * 1024;

/// Valid filter sizes.
pub const FILTER_SIZES: [usize; 3] = [FILTER_SIZE_S, FILTER_SIZE_M, FILTER_SIZE_L];

/// Target false positive rate of filter.
pub const FILTER_FP_RATE: f64 = 0.01;
/// Number of hashes used for bloom filter.
pub const FILTER_HASHES: usize = 7;

/// Inventory filter used for subscriptions and inventory comparison.
///
/// The [`Default`] instance has all bits set to `1`, ie. it will match
/// everything.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Filter(BloomFilter<RepoId>);

impl Default for Filter {
    fn default() -> Self {
        Self(BloomFilter::from(vec![0xff; FILTER_SIZE_S]))
    }
}

impl Filter {
    /// Create a new filter with the given items.
    ///
    /// Uses the iterator's size hint to determine the size of the filter.
    pub fn new(ids: impl IntoIterator<Item = RepoId>) -> Self {
        let iterator = ids.into_iter();
        let (min, _) = iterator.size_hint();
        let size = bloomy::bloom::optimal_bits(min, FILTER_FP_RATE) / 8;
        let size = if size > FILTER_SIZE_M {
            FILTER_SIZE_L
        } else if size > FILTER_SIZE_S {
            FILTER_SIZE_M
        } else {
            FILTER_SIZE_S
        };
        let mut bloom = BloomFilter::with_size(size);

        for id in iterator {
            bloom.insert(&id);
        }
        Self(bloom)
    }

    /// Empty filter with nothing set.
    pub fn empty() -> Self {
        Self(BloomFilter::from(vec![0x0; FILTER_SIZE_S]))
    }

    /// Size in bytes.
    pub fn size(&self) -> usize {
        self.0.bits() / 8
    }

    /// Check if the filter contains this repository ID
    pub fn contains(&self, rid: &RepoId) -> bool {
        self.0.contains(rid)
    }
}

impl Deref for Filter {
    type Target = BloomFilter<RepoId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Filter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<BloomFilter<RepoId>> for Filter {
    fn from(bloom: BloomFilter<RepoId>) -> Self {
        Self(bloom)
    }
}
