use std::io;
use std::ops::{Deref, DerefMut};

use bloomy::BloomFilter;

use crate::identity::Id;
use crate::protocol::wire;

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

#[cfg(test)]
impl From<BloomFilter<Id>> for Filter {
    fn from(bloom: BloomFilter<Id>) -> Self {
        Self(bloom)
    }
}

impl wire::Encode for Filter {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.0.as_bytes().encode(writer)?;

        Ok(n)
    }
}

impl wire::Decode for Filter {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let size: usize = wire::Decode::decode(reader)?;
        if size != FILTER_SIZE {
            return Err(wire::Error::InvalidFilterSize(size));
        }

        let bytes: [u8; FILTER_SIZE] = wire::Decode::decode(reader)?;
        let bf = BloomFilter::from(Vec::from(bytes));

        debug_assert_eq!(bf.hashes(), FILTER_HASHES);

        Ok(Self(bf))
    }
}
