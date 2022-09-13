use std::io;
use std::ops::{Deref, DerefMut};

use bloomy::BloomFilter;

use crate::identity::Id;
use crate::protocol::wire;

/// Size in bytes of subscription bloom filter.
pub const FILTER_SIZE: usize = 1024 * 16;
/// Number of hashes used for bloom filter.
pub const FILTER_HASHES: usize = 2;

/// Subscription filter.
/// Nb. This filter doesn't currently support inserting public keys.
#[derive(Clone, PartialEq, Eq)]
pub struct Filter(BloomFilter<Id>);

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

impl Default for Filter {
    fn default() -> Self {
        Self(BloomFilter::with_size(FILTER_SIZE))
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
        let bytes: [u8; FILTER_SIZE] = wire::Decode::decode(reader)?;
        let bf = BloomFilter::from(Vec::from(bytes));

        debug_assert_eq!(bf.hashes(), FILTER_HASHES);

        Ok(Self(bf))
    }
}
