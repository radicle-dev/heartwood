pub mod frame;
pub mod message;
pub mod varint;

pub use frame::StreamId;
pub use message::{AddressType, MessageType};

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::mem;
use std::ops::Deref;
use std::str::FromStr;
use std::string::FromUtf8Error;

use bytes::{Buf, BufMut};

use cyphernet::addr::tor;

use radicle::crypto::{PublicKey, Signature, Unverified};
use radicle::git;
use radicle::git::fmt;
use radicle::identity::RepoId;
use radicle::node;
use radicle::node::Alias;
use radicle::node::NodeId;
use radicle::node::Timestamp;
use radicle::node::UserAgent;
use radicle::storage::refs::Refs;
use radicle::storage::refs::RefsAt;
use radicle::storage::refs::SignedRefs;

use crate::bounded::BoundedVec;
use crate::service::filter;

/// The default type we use to represent sizes on the wire.
///
/// Since wire messages are limited to 64KB by the transport layer,
/// two bytes is enough to represent any message.
///
/// Note that in certain cases, we may use a smaller type.
pub type Size = u16;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("UTF-8 error: {0}")]
    FromUtf8(#[from] FromUtf8Error),
    #[error("invalid size: expected {expected}, got {actual}")]
    InvalidSize { expected: usize, actual: usize },
    #[error("invalid filter size: {0}")]
    InvalidFilterSize(usize),
    #[error("invalid channel type {0:x}")]
    InvalidStreamKind(u8),
    #[error(transparent)]
    InvalidRefName(#[from] fmt::Error),
    #[error(transparent)]
    InvalidAlias(#[from] node::AliasError),
    #[error("invalid user agent string: {0:?}")]
    InvalidUserAgent(String),
    #[error("invalid control message with type `{0}`")]
    InvalidControlMessage(u8),
    #[error("invalid protocol version header `{0:x?}`")]
    InvalidProtocolVersion([u8; 4]),
    #[error("invalid onion address: {0}")]
    InvalidOnionAddr(#[from] tor::OnionAddrDecodeError),
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(u64),
    #[error("wrong protocol version `{0}`")]
    WrongProtocolVersion(u8),
    #[error("unknown address type `{0}`")]
    UnknownAddressType(u8),
    #[error("unknown message type `{0}`")]
    UnknownMessageType(u16),
    #[error("unknown info type `{0}`")]
    UnknownInfoType(u16),
    #[error("unexpected bytes")]
    UnexpectedBytes,
    #[error("unexpected end of buffer, requested {requested} more bytes but only {available} are available")]
    UnexpectedEnd { available: usize, requested: usize },
}

impl From<bytes::TryGetError> for Error {
    fn from(
        bytes::TryGetError {
            available,
            requested,
        }: bytes::TryGetError,
    ) -> Self {
        Self::UnexpectedEnd {
            available,
            requested,
        }
    }
}

/// Things that can be encoded as binary.
pub trait Encode {
    fn encode(&self, buffer: &mut impl BufMut);
}

/// Things that can be decoded from binary.
pub trait Decode: Sized {
    fn decode(buffer: &mut impl Buf) -> Result<Self, Error>;
}

/// Encode an object into a byte vector.
pub fn serialize<E: Encode + ?Sized>(data: &E) -> Vec<u8> {
    let mut buffer = Vec::new();
    data.encode(&mut buffer);
    buffer
}

/// Decode an object from a slice.
pub fn deserialize<T: Decode>(mut data: &[u8]) -> Result<T, Error> {
    let result = T::decode(&mut data)?;

    if data.is_empty() {
        Ok(result)
    } else {
        Err(Error::UnexpectedBytes)
    }
}

impl Encode for u8 {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u8(*self);
    }
}

impl Encode for u16 {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u16(*self);
    }
}

impl Encode for u32 {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u32(*self);
    }
}

impl Encode for u64 {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u64(*self);
    }
}

impl Encode for PublicKey {
    fn encode(&self, buf: &mut impl BufMut) {
        self.deref().encode(buf)
    }
}

impl<const T: usize> Encode for &[u8; T] {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_slice(&**self);
    }
}

impl<const T: usize> Encode for [u8; T] {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_slice(self);
    }
}

impl<T> Encode for &[T]
where
    T: Encode,
{
    fn encode(&self, buf: &mut impl BufMut) {
        (self.len() as Size).encode(buf);

        for item in self.iter() {
            item.encode(buf);
        }
    }
}

impl<T, const N: usize> Encode for BoundedVec<T, N>
where
    T: Encode,
{
    fn encode(&self, buf: &mut impl BufMut) {
        self.as_slice().encode(buf)
    }
}

impl Encode for &str {
    fn encode(&self, buf: &mut impl BufMut) {
        assert!(self.len() <= u8::MAX as usize);

        (self.len() as u8).encode(buf);
        let bytes = self.as_bytes();

        // Nb. Don't use the [`Encode`] instance here for &[u8], because we are prefixing the
        // length ourselves.
        buf.put_slice(bytes);
    }
}

impl Encode for String {
    fn encode(&self, buf: &mut impl BufMut) {
        self.as_str().encode(buf)
    }
}

impl Encode for git::Url {
    fn encode(&self, buf: &mut impl BufMut) {
        self.to_string().encode(buf)
    }
}

impl Encode for RepoId {
    fn encode(&self, buf: &mut impl BufMut) {
        self.deref().encode(buf)
    }
}

impl Encode for Refs {
    fn encode(&self, buf: &mut impl BufMut) {
        let len: Size = self
            .len()
            .try_into()
            .expect("`Refs::len()` must be less than or equal to `Size::MAX`");
        len.encode(buf);

        for (name, oid) in self.iter() {
            name.as_str().encode(buf);
            oid.encode(buf);
        }
    }
}

impl Encode for cyphernet::addr::tor::OnionAddrV3 {
    fn encode(&self, buf: &mut impl BufMut) {
        self.into_raw_bytes().encode(buf)
    }
}

impl Encode for UserAgent {
    fn encode(&self, buf: &mut impl BufMut) {
        self.as_ref().encode(buf)
    }
}

impl Encode for Alias {
    fn encode(&self, buf: &mut impl BufMut) {
        self.as_ref().encode(buf)
    }
}

impl<A, B> Encode for (A, B)
where
    A: Encode,
    B: Encode,
{
    fn encode(&self, buf: &mut impl BufMut) {
        self.0.encode(buf);
        self.1.encode(buf);
    }
}

impl Encode for git::RefString {
    fn encode(&self, buf: &mut impl BufMut) {
        self.as_str().encode(buf)
    }
}

impl Encode for Signature {
    fn encode(&self, buf: &mut impl BufMut) {
        self.deref().encode(buf)
    }
}

impl Encode for git::Oid {
    fn encode(&self, buf: &mut impl BufMut) {
        // Nb. We use length-encoding here to support future SHA-2 object ids.
        self.as_bytes().encode(buf)
    }
}

////////////////////////////////////////////////////////////////////////////////

impl Decode for PublicKey {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let buf: [u8; 32] = Decode::decode(buf)?;

        Ok(PublicKey::from(buf))
    }
}

impl Decode for Refs {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let len = Size::decode(buf)?;
        let mut refs = BTreeMap::new();

        for _ in 0..len {
            let name = String::decode(buf)?;
            let name = git::RefString::try_from(name).map_err(Error::from)?;
            let oid = git::Oid::decode(buf)?;

            refs.insert(name, oid);
        }
        Ok(refs.into())
    }
}

impl Decode for git::RefString {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let ref_str = String::decode(buf)?;
        git::RefString::try_from(ref_str).map_err(Error::from)
    }
}

impl Decode for UserAgent {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        String::decode(buf).and_then(|s| UserAgent::from_str(&s).map_err(Error::InvalidUserAgent))
    }
}

impl Decode for Alias {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        String::decode(buf).and_then(|s| Alias::from_str(&s).map_err(Error::from))
    }
}

impl<A, B> Decode for (A, B)
where
    A: Decode,
    B: Decode,
{
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let a = A::decode(buf)?;
        let b = B::decode(buf)?;
        Ok((a, b))
    }
}

impl Decode for git::Oid {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let len = Size::decode(buf)? as usize;
        #[allow(non_upper_case_globals)]
        const expected: usize = mem::size_of::<git::raw::Oid>();

        if len != expected {
            return Err(Error::InvalidSize {
                expected,
                actual: len,
            });
        }

        let buf: [u8; expected] = Decode::decode(buf)?;
        let oid = git::raw::Oid::from_bytes(&buf).expect("the buffer is exactly the right size");
        let oid = git::Oid::from(oid);

        Ok(oid)
    }
}

impl Decode for Signature {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let bytes: [u8; 64] = Decode::decode(buf)?;

        Ok(Signature::from(bytes))
    }
}

impl Decode for u8 {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        Ok(buf.try_get_u8()?)
    }
}

impl Decode for u16 {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        Ok(buf.try_get_u16()?)
    }
}

impl Decode for u32 {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        Ok(buf.try_get_u32()?)
    }
}

impl Decode for u64 {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        Ok(buf.try_get_u64()?)
    }
}

impl<const N: usize> Decode for [u8; N] {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let mut ary = [0; N];
        buf.try_copy_to_slice(&mut ary).map_err(Error::from)?;

        Ok(ary)
    }
}

impl<T, const N: usize> Decode for BoundedVec<T, N>
where
    T: Decode,
{
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let len: usize = Size::decode(buf)? as usize;
        let mut items = Self::with_capacity(len).map_err(|_| Error::InvalidSize {
            expected: Self::max(),
            actual: len,
        })?;

        for _ in 0..items.capacity() {
            let item = T::decode(buf)?;
            items.push(item).ok();
        }
        Ok(items)
    }
}

impl Decode for String {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let len = u8::decode(buf)?;
        let mut bytes = vec![0; len as usize];

        buf.try_copy_to_slice(&mut bytes)?;

        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

impl Decode for RepoId {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let oid: git::Oid = Decode::decode(buf)?;

        Ok(Self::from(oid))
    }
}

impl Encode for filter::Filter {
    fn encode(&self, buf: &mut impl BufMut) {
        self.deref().as_bytes().encode(buf);
    }
}

impl Decode for filter::Filter {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let size: usize = Size::decode(buf)? as usize;
        if !filter::FILTER_SIZES.contains(&size) {
            return Err(Error::InvalidFilterSize(size));
        }

        let mut bytes = vec![0; size];

        buf.try_copy_to_slice(&mut bytes)?;

        let f = filter::BloomFilter::from(bytes);
        debug_assert_eq!(f.hashes(), filter::FILTER_HASHES);

        Ok(Self::from(f))
    }
}

impl<V> Encode for SignedRefs<V> {
    fn encode(&self, buf: &mut impl BufMut) {
        self.id.encode(buf);
        self.refs.encode(buf);
        self.signature.encode(buf);
    }
}

impl Decode for SignedRefs<Unverified> {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let id = NodeId::decode(buf)?;
        let refs = Refs::decode(buf)?;
        let signature = Signature::decode(buf)?;

        Ok(Self::new(refs, id, signature))
    }
}

impl Encode for RefsAt {
    fn encode(&self, buf: &mut impl BufMut) {
        self.remote.encode(buf);
        self.at.encode(buf);
    }
}

impl Decode for RefsAt {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let remote = NodeId::decode(buf)?;
        let at = git::Oid::decode(buf)?;
        Ok(Self { remote, at })
    }
}

impl Encode for node::Features {
    fn encode(&self, buf: &mut impl BufMut) {
        self.deref().encode(buf)
    }
}

impl Decode for node::Features {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let features = u64::decode(buf)?;

        Ok(Self::from(features))
    }
}

impl Decode for tor::OnionAddrV3 {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let bytes: [u8; tor::ONION_V3_RAW_LEN] = Decode::decode(buf)?;
        let addr = tor::OnionAddrV3::from_raw_bytes(bytes)?;

        Ok(addr)
    }
}

impl Encode for Timestamp {
    fn encode(&self, buf: &mut impl BufMut) {
        self.deref().encode(buf)
    }
}

impl Decode for Timestamp {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let millis = u64::decode(buf)?;
        let ts = Timestamp::try_from(millis).map_err(Error::InvalidTimestamp)?;

        Ok(ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcheck;
    use qcheck_macros::quickcheck;

    use radicle::assert_matches;
    use radicle::crypto::Unverified;
    use radicle::storage::refs::SignedRefs;

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
    fn prop_id(input: RepoId) {
        assert_eq!(deserialize::<RepoId>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_refs(input: Refs) {
        assert_eq!(deserialize::<Refs>(&serialize(&input)).unwrap(), input);
    }

    #[quickcheck]
    fn prop_tuple(input: (String, String)) {
        assert_eq!(
            deserialize::<(String, String)>(&serialize(&input)).unwrap(),
            input
        );
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
    fn test_alias() {
        assert_eq!(
            serialize(&Alias::from_str("hello").unwrap()),
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
