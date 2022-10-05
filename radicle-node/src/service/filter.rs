#![allow(clippy::identity_op)]
use std::ops::{Deref, DerefMut};

pub use bloomy::BloomFilter;

use crate::identity::Id;

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
pub struct Filter(BloomFilter<Id>);

impl Default for Filter {
    fn default() -> Self {
        Self(BloomFilter::from(vec![0xff; FILTER_SIZE_S]))
    }
}

impl Filter {
    /// Create a new filter with the given items.
    ///
    /// Uses the iterator's size hint to determine the size of the filter.
    pub fn new<'a>(ids: impl IntoIterator<Item = &'a Id>) -> Self {
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
            bloom.insert(id);
        }
        Self(bloom)
    }

    /// Size in bytes.
    pub fn size(&self) -> usize {
        self.0.bits() / 8
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::arbitrary;

    #[test]
    fn test_parameters() {
        // To store 10'000 items with a false positive rate of 1%, we need about 12KB.
        assert_eq!(bloomy::bloom::optimal_bits(10_000, 0.01) / 8, 11_981);
        // To store 1'000 items with a false positive rate of 1%, we need about 1KB.
        assert_eq!(bloomy::bloom::optimal_bits(1_000, 0.01) / 8, 1198);
        // To store 100 items with a false positive rate of 1%, we need about 120B.
        assert_eq!(bloomy::bloom::optimal_bits(100, 0.01) / 8, 119);

        // With 16KB, we can store 13'675 items with a false positive rate of 1%.
        assert_eq!(
            bloomy::bloom::optimal_capacity(FILTER_SIZE_L * 8, FILTER_FP_RATE),
            13_675
        );
        // With 4KB, we can store 3'419 items with a false positive rate of 1%.
        assert_eq!(
            bloomy::bloom::optimal_capacity(FILTER_SIZE_M * 8, FILTER_FP_RATE),
            3419
        );
        // With 1KB, we can store 855 items with a false positive rate of 1%.
        assert_eq!(
            bloomy::bloom::optimal_capacity(FILTER_SIZE_S * 8, FILTER_FP_RATE),
            855
        );

        assert_eq!(
            bloomy::bloom::optimal_hashes(FILTER_SIZE_L * 8, 13_675),
            FILTER_HASHES
        );
        assert_eq!(
            bloomy::bloom::optimal_hashes(FILTER_SIZE_M * 8, 3419),
            FILTER_HASHES
        );
        assert_eq!(
            bloomy::bloom::optimal_hashes(FILTER_SIZE_S * 8, 855),
            FILTER_HASHES
        );
    }

    #[test]
    fn test_sizes() {
        let ids = arbitrary::vec::<Id>(3420);
        let f = Filter::new(ids.iter().take(10));
        assert_eq!(f.size(), FILTER_SIZE_S);

        let f = Filter::new(ids.iter().take(1000));
        assert_eq!(f.size(), FILTER_SIZE_M);

        let f = Filter::new(ids.iter());
        assert_eq!(f.size(), FILTER_SIZE_L);

        // Just checking that iterators over hash sets give correct size hints.
        let hs = arbitrary::set::<Id>(42..=42);
        assert_eq!(hs.iter().size_hint(), (42, Some(42)));
    }
}
