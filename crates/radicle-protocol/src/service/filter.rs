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

    pub fn allowed_by(
        policies: impl Iterator<
            Item = Result<radicle::node::policy::SeedPolicy, radicle::node::policy::store::Error>,
        >,
    ) -> Self {
        let mut ids = Vec::new();

        for seed in policies {
            let seed = match seed {
                Ok(seed) => seed,
                Err(err) => {
                    log::error!(target: "protocol::filter", "Failed to read seed policy: {err}");
                    continue;
                }
            };

            if seed.policy.is_allow() {
                ids.push(seed.rid);
            }
        }

        Self::new(ids)
    }

    /// Empty filter with nothing set.
    pub fn empty() -> Self {
        Self(BloomFilter::from(vec![0x0; FILTER_SIZE_S]))
    }

    /// Size in bytes.
    pub fn size(&self) -> usize {
        self.0.bits() / 8
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

#[allow(clippy::unwrap_used)]
#[cfg(any(test, feature = "test"))]
impl qcheck::Arbitrary for Filter {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let size = *g
            .choose(&[FILTER_SIZE_S, FILTER_SIZE_M, FILTER_SIZE_L])
            .unwrap();
        let mut bytes = vec![0; size];
        for _ in 0..64 {
            let index = usize::arbitrary(g) % bytes.len();
            bytes[index] = u8::arbitrary(g);
        }
        Self::from(BloomFilter::from(bytes))
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use super::*;
    use radicle::test::arbitrary;

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
        let ids = arbitrary::vec::<RepoId>(3420);
        let f = Filter::new(ids.iter().cloned().take(10));
        assert_eq!(f.size(), FILTER_SIZE_S);

        let f = Filter::new(ids.iter().cloned().take(1000));
        assert_eq!(f.size(), FILTER_SIZE_M);

        let f = Filter::new(ids.iter().cloned());
        assert_eq!(f.size(), FILTER_SIZE_L);

        // Just checking that iterators over hash sets give correct size hints.
        let hs = arbitrary::set::<RepoId>(42..=42);
        assert_eq!(hs.iter().size_hint(), (42, Some(42)));
    }

    /// Checks that a particular filter extracted from a live deployment of
    /// `radicle-node` at `release/1.5.0`, which is known to contain
    /// "heartwood", actually also evaluates to contain "heartwood".
    ///
    /// This is to catch regressions in the [`std::hash::Hash`] implementation
    /// [`RepoId`] and other breaking changes to [`Filter`].
    #[test]
    fn compatible() {
        let filter = {
            let mut filter = [0u8; FILTER_SIZE_S];

            #[rustfmt::skip]
            const COMPRESSED_FIXTURE: [(usize, u8); 100] = [
                (0x002, 0xa8), (0x010, 0x08), (0x016, 0x40), (0x01b, 0x20), (0x04d, 0x04),
                (0x050, 0x04), (0x05a, 0x02), (0x05e, 0x80), (0x06d, 0x40), (0x075, 0x08),
                (0x082, 0x80), (0x084, 0x80), (0x089, 0x01), (0x08b, 0x08), (0x099, 0x04),
                (0x0a1, 0x40), (0x0a7, 0x40), (0x0be, 0x40), (0x0d3, 0x01), (0x0e2, 0x01),
                (0x0ee, 0x08), (0x0f2, 0x04), (0x109, 0x08), (0x119, 0x10), (0x15b, 0x40),
                (0x160, 0x44), (0x163, 0x01), (0x168, 0x08), (0x16b, 0x01), (0x16d, 0x04),
                (0x176, 0x80), (0x17e, 0x40), (0x189, 0x20), (0x18f, 0x04), (0x19f, 0x20),
                (0x1b2, 0x08), (0x1b5, 0x04), (0x1b8, 0x20), (0x1ed, 0x10), (0x1f1, 0x40),
                (0x1f3, 0x04), (0x1fa, 0x40), (0x20b, 0x08), (0x20e, 0x04), (0x218, 0x01),
                (0x231, 0x02), (0x23d, 0x80), (0x248, 0x10), (0x24e, 0x04), (0x250, 0x01),
                (0x251, 0x01), (0x255, 0x04), (0x25a, 0x10), (0x265, 0x20), (0x27c, 0x01),
                (0x284, 0x04), (0x285, 0x20), (0x28d, 0x81), (0x29f, 0x01), (0x2a6, 0x10),
                (0x2ac, 0x40), (0x2ad, 0x10), (0x2b4, 0x04), (0x2b8, 0x02), (0x2cb, 0x01),
                (0x2d1, 0x80), (0x2d4, 0x01), (0x2d7, 0x40), (0x2ed, 0x80), (0x2f7, 0x01),
                (0x302, 0x80), (0x303, 0x40), (0x307, 0x40), (0x309, 0x04), (0x318, 0x04),
                (0x31e, 0x10), (0x335, 0x01), (0x336, 0x40), (0x338, 0x40), (0x351, 0x80),
                (0x353, 0x10), (0x359, 0x0c), (0x360, 0x40), (0x367, 0x01), (0x36b, 0x08),
                (0x36c, 0x40), (0x37b, 0x10), (0x37d, 0x40), (0x399, 0x02), (0x39f, 0x02),
                (0x3a6, 0x02), (0x3a9, 0x04), (0x3ab, 0x01), (0x3cb, 0x04), (0x3e2, 0x01),
                (0x3e5, 0x10), (0x3ea, 0x40), (0x3ed, 0x40), (0x3f2, 0x02), (0x3f5, 0x80),
            ];

            for (i, v) in COMPRESSED_FIXTURE.into_iter() {
                filter[i] = v;
            }

            Filter(BloomFilter::from(filter.to_vec()))
        };

        assert!(filter.contains(&"rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5".parse().unwrap()),);
    }
}
