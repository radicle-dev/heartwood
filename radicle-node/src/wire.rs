pub mod message;
mod old;

use std::collections::{BTreeMap, HashMap, VecDeque};
pub use old::Wire;

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::ops::Deref;
use std::string::FromUtf8Error;
use std::{io, mem};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};

use crate::crypto::hash::Digest;
use crate::crypto::{PublicKey, Signature, Unverified};
use crate::git;
use crate::git::fmt;
use crate::identity::Id;
use crate::node;
use crate::prelude::*;
use crate::service::filter;
use crate::storage::refs::Refs;
use crate::storage::refs::SignedRefs;

/// The default type we use to represent sizes on the wire.
///
/// Since wire messages are limited to 64KB by the transport layer,
/// two bytes is enough to represent any message.
///
/// Note that in certain cases, we may use a smaller type.
pub type Size = u16;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("UTF-8 error: {0}")]
    FromUtf8(#[from] FromUtf8Error),
    #[error("invalid size: expected {expected}, got {actual}")]
    InvalidSize { expected: usize, actual: usize },
    #[error("invalid filter size: {0}")]
    InvalidFilterSize(usize),
    #[error(transparent)]
    InvalidRefName(#[from] fmt::Error),
    #[error("unknown address type `{0}`")]
    UnknownAddressType(u8),
    #[error("unknown message type `{0}`")]
    UnknownMessageType(u16),
}

impl Error {
    /// Whether we've reached the end of file. This will be true when we fail to decode
    /// a message because there's not enough data in the stream.
    pub fn is_eof(&self) -> bool {
        matches!(self, Self::Io(err) if err.kind() == io::ErrorKind::UnexpectedEof)
    }
}

/// Things that can be encoded as binary.
pub trait Encode {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error>;
}

/// Things that can be decoded from binary.
pub trait Decode: Sized {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error>;
}

/// Encode an object into a vector.
pub fn serialize<T: Encode + ?Sized>(data: &T) -> Vec<u8> {
    let mut buffer = Vec::new();
    let len = data
        .encode(&mut buffer)
        .expect("in-memory writes don't error");

    debug_assert_eq!(len, buffer.len());

    buffer
}

/// Decode an object from a vector.
pub fn deserialize<T: Decode>(data: &[u8]) -> Result<T, Error> {
    let mut cursor = io::Cursor::new(data);

    T::decode(&mut cursor)
}

impl Encode for u8 {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_u8(*self)?;

        Ok(mem::size_of::<Self>())
    }
}

impl Encode for u16 {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_u16::<NetworkEndian>(*self)?;

        Ok(mem::size_of::<Self>())
    }
}

impl Encode for u32 {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_u32::<NetworkEndian>(*self)?;

        Ok(mem::size_of::<Self>())
    }
}

impl Encode for u64 {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_u64::<NetworkEndian>(*self)?;

        Ok(mem::size_of::<Self>())
    }
}

impl Encode for PublicKey {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.deref().encode(writer)
    }
}

impl<const T: usize> Encode for &[u8; T] {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        // TODO: This can be removed when the clippy bugs are fixed
        #[allow(clippy::explicit_auto_deref)]
        writer.write_all(*self)?;

        Ok(mem::size_of::<Self>())
    }
}

impl<const T: usize> Encode for [u8; T] {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(self)?;

        Ok(mem::size_of::<Self>())
    }
}

impl<T> Encode for &[T]
where
    T: Encode,
{
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = (self.len() as Size).encode(writer)?;

        for item in self.iter() {
            n += item.encode(writer)?;
        }
        Ok(n)
    }
}

impl<T, const N: usize> Encode for BoundedVec<T, N>
where
    T: Encode,
{
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.as_slice().encode(writer)
    }
}

impl Encode for &str {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        assert!(self.len() <= u8::MAX as usize);

        let n = (self.len() as u8).encode(writer)?;
        let bytes = self.as_bytes();

        // Nb. Don't use the [`Encode`] instance here for &[u8], because we are prefixing the
        // length ourselves.
        writer.write_all(bytes)?;

        Ok(n + bytes.len())
    }
}

impl Encode for String {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.as_str().encode(writer)
    }
}

impl Encode for git::Url {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.to_string().encode(writer)
    }
}

impl Encode for Digest {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.as_ref().encode(writer)
    }
}

impl Encode for Id {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.deref().encode(writer)
    }
}

impl Encode for Refs {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let len: Size = self
            .len()
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut n = len.encode(writer)?;

        for (name, oid) in self.iter() {
            n += name.as_str().encode(writer)?;
            n += oid.encode(writer)?;
        }
        Ok(n)
    }
}

impl Encode for Signature {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.deref().encode(writer)
    }
}

impl Encode for git::Oid {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        // Nb. We use length-encoding here to support future SHA-2 object ids.
        self.as_bytes().encode(writer)
    }
}

////////////////////////////////////////////////////////////////////////////////

impl Decode for PublicKey {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let buf: [u8; 32] = Decode::decode(reader)?;

        PublicKey::try_from(buf)
            .map_err(|e| Error::Io(io::Error::new(io::ErrorKind::InvalidInput, e.to_string())))
    }
}

impl Decode for Refs {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let len = Size::decode(reader)?;
        let mut refs = BTreeMap::new();

        for _ in 0..len {
            let name = String::decode(reader)?;
            let name = git::RefString::try_from(name).map_err(Error::from)?;
            let oid = git::Oid::decode(reader)?;

            refs.insert(name, oid);
        }
        Ok(refs.into())
    }
}

impl Decode for git::Oid {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let len = Size::decode(reader)? as usize;
        #[allow(non_upper_case_globals)]
        const expected: usize = mem::size_of::<git::raw::Oid>();

        if len != expected {
            return Err(Error::InvalidSize {
                expected,
                actual: len,
            });
        }

        let buf: [u8; expected] = Decode::decode(reader)?;
        let oid = git::raw::Oid::from_bytes(&buf).expect("the buffer is exactly the right size");
        let oid = git::Oid::from(oid);

        Ok(oid)
    }
}

impl Decode for Signature {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let bytes: [u8; 64] = Decode::decode(reader)?;

        Ok(Signature::from(bytes))
    }
}

impl Decode for u8 {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        reader.read_u8().map_err(Error::from)
    }
}

impl Decode for u16 {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        reader.read_u16::<NetworkEndian>().map_err(Error::from)
    }
}

impl Decode for u32 {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        reader.read_u32::<NetworkEndian>().map_err(Error::from)
    }
}

impl Decode for u64 {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        reader.read_u64::<NetworkEndian>().map_err(Error::from)
    }
}

impl<const N: usize> Decode for [u8; N] {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let mut ary = [0; N];
        reader.read_exact(&mut ary)?;

        Ok(ary)
    }
}

impl<T, const N: usize> Decode for BoundedVec<T, N>
where
    T: Decode,
{
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let len: usize = Size::decode(reader)? as usize;
        let mut items = Self::with_capacity(len).map_err(|_| Error::InvalidSize {
            expected: Self::max(),
            actual: len,
        })?;

        for _ in 0..items.capacity() {
            let item = T::decode(reader)?;
            items.push(item).ok();
        }
        Ok(items)
    }
}

impl Decode for String {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let len = u8::decode(reader)?;
        let mut bytes = vec![0; len as usize];

        reader.read_exact(&mut bytes)?;

        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

impl Decode for Id {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let oid: git::Oid = Decode::decode(reader)?;

        Ok(Self::from(oid))
    }
}

impl Decode for Digest {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let bytes: [u8; 32] = Decode::decode(reader)?;

        Ok(Self::from(bytes))
    }
}

impl Encode for filter::Filter {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.deref().as_bytes().encode(writer)?;

        Ok(n)
    }
}

impl Decode for filter::Filter {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let size: usize = Size::decode(reader)? as usize;
        if !filter::FILTER_SIZES.contains(&size) {
            return Err(Error::InvalidFilterSize(size));
        }

        let mut bytes = vec![0; size];
        reader.read_exact(&mut bytes[..])?;

        let f = filter::BloomFilter::from(bytes);
        debug_assert_eq!(f.hashes(), filter::FILTER_HASHES);

        Ok(Self::from(f))
    }
}

impl<V> Encode for SignedRefs<V> {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.refs.encode(writer)?;
        n += self.signature.encode(writer)?;

        Ok(n)
    }
}

impl Decode for SignedRefs<Unverified> {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let refs = Refs::decode(reader)?;
        let signature = Signature::decode(reader)?;

        Ok(Self::new(refs, signature))
    }
}

impl Encode for node::Features {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.deref().encode(writer)
    }
}

impl Decode for node::Features {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let features = u64::decode(reader)?;

        Ok(Self::from(features))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcheck_macros::quickcheck;

    use crate::crypto::Unverified;
    use crate::storage::refs::SignedRefs;
    use crate::test::assert_matches;

    #[quickcheck]
    fn prop_u8(input: u8) {
        assert_eq!(deserialize::<u8>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_u16(input: u16) {
        assert_eq!(deserialize::<u16>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_u32(input: u32) {
        assert_eq!(deserialize::<u32>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_u64(input: u64) {
        assert_eq!(deserialize::<u64>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_string(input: String) -> qcheck::TestResult {
        if input.len() > u8::MAX as usize {
            return qcheck::TestResult::discard();
        }
        assert_eq!(deserialize::<String>(&serialize(&input)).unwrap(), input);

        qcheck::TestResult::passed()
    }

    #[quickcheck]
    fn prop_vec(input: BoundedVec<String, 16>) {
        assert_eq!(
            deserialize::<BoundedVec<String, 16>>(&serialize(&input.as_slice())).unwrap(),
            input
        );
    }

    #[quickcheck]
    fn prop_pubkey(input: PublicKey) {
        assert_eq!(deserialize::<PublicKey>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_filter(input: filter::Filter) {
        assert_eq!(
            deserialize::<filter::Filter>(&serialize(&input)).unwrap(),
            input
        );
    }

    #[quickcheck]
    fn prop_id(input: Id) {
        assert_eq!(deserialize::<Id>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_digest(input: Digest) {
        assert_eq!(deserialize::<Digest>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_refs(input: Refs) {
        assert_eq!(deserialize::<Refs>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_signature(input: [u8; 64]) {
        let signature = Signature::from(input);

        assert_eq!(
            deserialize::<Signature>(&serialize(&signature)).unwrap(),
            signature
        );
    }

    #[quickcheck]
    fn prop_oid(input: [u8; 20]) {
        let oid = git::Oid::try_from(input.as_slice()).unwrap();

        assert_eq!(deserialize::<git::Oid>(&serialize(&oid)).unwrap(), oid);
    }

    #[quickcheck]
    fn prop_signed_refs(input: SignedRefs<Unverified>) {
        assert_eq!(
            deserialize::<SignedRefs<Unverified>>(&serialize(&input)).unwrap(),
            input
        );
    }

    #[test]
    fn test_string() {
        assert_eq!(
            serialize(&String::from("hello")),
            vec![5, b'h', b'e', b'l', b'l', b'o']
        );
    }

    #[test]
    fn test_filter_invalid() {
        let b = bloomy::BloomFilter::with_size(filter::FILTER_SIZE_M / 3);
        let f = filter::Filter::from(b);
        let bytes = serialize(&f);

        assert_matches!(
            deserialize::<filter::Filter>(&bytes).unwrap_err(),
            Error::InvalidFilterSize(_)
        );
    }

    #[test]
    fn test_bounded_vec_limit() {
        let v: BoundedVec<u8, 2> = vec![1, 2].try_into().unwrap();
        let buf = serialize(&v);

        assert_matches!(
            deserialize::<BoundedVec<u8, 1>>(&buf),
            Err(Error::InvalidSize {
                expected: 1,
                actual: 2
            }),
            "fail when vector is too small for buffer",
        );

        assert!(
            deserialize::<BoundedVec<u8, 2>>(&buf).is_ok(),
            "successfully decode vector of same size",
        );
    }
}
