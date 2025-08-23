pub mod frame;
pub mod message;
pub mod varint;

pub use frame::StreamId;
pub use message::{AddressType, MessageType};

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::fmt::Debug;
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
pub enum Invalid {
    #[error("invalid Git object identifier size: expected {expected}, got {actual}")]
    Oid { expected: usize, actual: usize },
    #[error(transparent)]
    Bounded(#[from] crate::bounded::Error),
    #[error("invalid filter size: {actual}")]
    FilterSize { actual: usize },
    #[error("UTF-8 error: {0}")]
    FromUtf8(#[from] FromUtf8Error),
    #[error(transparent)]
    RefName(#[from] fmt::Error),
    #[error(transparent)]
    Alias(#[from] node::AliasError),
    #[error("invalid user agent string: {err}")]
    InvalidUserAgent { err: String },
    #[error("invalid onion address: {0}")]
    OnionAddr(#[from] tor::OnionAddrDecodeError),
    #[error("invalid timestamp: {actual_millis} millis")]
    Timestamp { actual_millis: u64 },

    // Message types
    #[error("invalid control message type: {actual:x}")]
    ControlType { actual: u8 },
    #[error("invalid stream type: {actual:x}")]
    StreamType { actual: u8 },
    #[error("invalid address type: {actual:x}")]
    AddressType { actual: u8 },
    #[error("invalid message type: {actual:x}")]
    MessageType { actual: u16 },
    #[error("invalid info message type: {actual:x}")]
    InfoMessageType { actual: u16 },

    // Protocol version handling
    #[error("invalid protocol version string: {actual:x?}")]
    ProtocolVersion { actual: [u8; 4] },
    #[error("unsupported protocol version: {actual}")]
    ProtocolVersionUnsupported { actual: u8 },
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Invalid(#[from] Invalid),

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
    /// Encode self by writing it to the given buffer.
    fn encode(&self, buffer: &mut impl BufMut);

    /// A convenience wrapper around [`Encode::encode`]
    /// that allocates a [`Vec`].
    fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode(&mut buf);
        buf
    }
}

/// Things that can be decoded from binary.
pub trait Decode: Sized {
    fn decode(buffer: &mut impl Buf) -> Result<Self, Error>;

    /// A convenience wrapper around [`Decode::decode`] to decode
    /// from a slice exactly.
    ///
    /// # Panics
    ///
    ///  - If decoding failed because there were not enough bytes.
    ///  - If there are any bytes left after decoding.
    #[cfg(test)]
    fn decode_exact(mut data: &[u8]) -> Result<Self, Invalid> {
        match Self::decode(&mut data) {
            Ok(value) => {
                if !data.is_empty() {
                    panic!("{} bytes left in buffer", data.len());
                }
                Ok(value)
            }
            Err(err @ Error::UnexpectedEnd { .. }) => {
                panic!("{}", err);
            }
            Err(Error::Invalid(e)) => Err(e),
        }
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
            let name = git::RefString::try_from(name).map_err(Invalid::from)?;
            let oid = git::Oid::decode(buf)?;

            refs.insert(name, oid);
        }
        Ok(refs.into())
    }
}

impl Decode for git::RefString {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let ref_str = String::decode(buf)?;
        Ok(git::RefString::try_from(ref_str).map_err(Invalid::from)?)
    }
}

impl Decode for UserAgent {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let user_agent = String::decode(buf)?;
        Ok(UserAgent::from_str(&user_agent).map_err(|err| Invalid::InvalidUserAgent { err })?)
    }
}

impl Decode for Alias {
    fn decode(buf: &mut impl Buf) -> Result<Self, Error> {
        let alias = String::decode(buf)?;
        Ok(Alias::from_str(&alias).map_err(Invalid::from)?)
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
        const LEN_EXPECTED: usize = mem::size_of::<git::raw::Oid>();

        let len = Size::decode(buf)? as usize;

        if len != LEN_EXPECTED {
            return Err(Invalid::Oid {
                expected: LEN_EXPECTED,
                actual: len,
            }
            .into());
        }

        let buf: [u8; LEN_EXPECTED] = Decode::decode(buf)?;
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
        let mut items = Self::with_capacity(len).map_err(Invalid::from)?;

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

        let string = String::from_utf8(bytes).map_err(Invalid::from)?;

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
            return Err(Invalid::FilterSize { actual: size }.into());
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
        let addr = tor::OnionAddrV3::from_raw_bytes(bytes).map_err(Invalid::from)?;

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
        let ts = Timestamp::try_from(millis).map_err(|value| Invalid::Timestamp {
            actual_millis: value,
        })?;

        Ok(ts)
    }
}

#[cfg(test)]
fn roundtrip<T>(value: T)
where
    T: Encode + Decode + PartialEq + Debug,
{
    let encoded = value.encode_to_vec();
    assert_eq!(T::decode_exact(&encoded).expect("roundtrip"), value);
}

#[cfg(test)]
#[macro_export]
macro_rules! prop_roundtrip {
    ($t:ty, $name:tt) => {
        paste::paste! {
            #[quickcheck]
            fn [< prop_roundtrip_ $name:lower >](v: $t) {
                $crate::wire::roundtrip(v);
            }
        }
    };
    ($t:ty) => {
        paste::paste! {
            prop_roundtrip!($t, [< $t >]);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    use qcheck;
    use qcheck_macros::quickcheck;

    use radicle::assert_matches;
    use radicle::crypto::Unverified;
    use radicle::storage::refs::SignedRefs;

    prop_roundtrip!(u16);
    prop_roundtrip!(u32);
    prop_roundtrip!(u64);
    prop_roundtrip!(BoundedVec<u8, 16>, vec);
    prop_roundtrip!(PublicKey);
    prop_roundtrip!(filter::Filter, filter);
    prop_roundtrip!(RepoId);
    prop_roundtrip!(Refs);
    prop_roundtrip!((String, String), tuple);
    prop_roundtrip!(SignedRefs<Unverified>, signed_refs);

    #[quickcheck]
    fn prop_string(input: String) -> qcheck::TestResult {
        if input.len() > u8::MAX as usize {
            return qcheck::TestResult::discard();
        }

        roundtrip(input);

        qcheck::TestResult::passed()
    }

    #[quickcheck]
    fn prop_signature(input: [u8; 64]) {
        roundtrip(Signature::from(input));
    }

    #[quickcheck]
    fn prop_oid(input: [u8; 20]) {
        roundtrip(git::Oid::try_from(input.as_slice()).unwrap());
    }

    #[test]
    fn test_string() {
        assert_eq!(
            String::from("hello").encode_to_vec(),
            vec![5, b'h', b'e', b'l', b'l', b'o']
        );
    }

    #[test]
    fn test_alias() {
        assert_eq!(
            Alias::from_str("hello").unwrap().encode_to_vec(),
            vec![5, b'h', b'e', b'l', b'l', b'o']
        );
    }

    #[test]
    fn test_filter_invalid() {
        let b = bloomy::BloomFilter::with_size(filter::FILTER_SIZE_M / 3);
        let f = filter::Filter::from(b);
        let bytes = f.encode_to_vec();

        assert_matches!(
            filter::Filter::decode_exact(&bytes).unwrap_err(),
            Invalid::FilterSize { .. }
        );
    }

    #[test]
    fn test_bounded_vec_limit() {
        let v: BoundedVec<u8, 2> = vec![1, 2].try_into().unwrap();
        let buf = &v.encode_to_vec();

        assert_matches!(
            BoundedVec::<u8, 1>::decode_exact(buf),
            Err(Invalid::Bounded(crate::bounded::Error::InvalidSize {
                expected: 1,
                actual: 2
            })),
            "fail when vector is too small for buffer",
        );

        assert!(
            BoundedVec::<u8, 2>::decode_exact(buf).is_ok(),
            "successfully decode vector of same size",
        );
    }
}
