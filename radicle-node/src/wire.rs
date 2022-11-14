pub mod message;
pub mod transcoder;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::convert::TryFrom;
use std::net;
use std::ops::{Deref, DerefMut};
use std::string::FromUtf8Error;
use std::{io, mem};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use nakamoto_net as nakamoto;
use nakamoto_net::Link;

use crate::address;
use crate::crypto::{PublicKey, Signature, Signer, Unverified};
use crate::git;
use crate::git::fmt;
use crate::hash::Digest;
use crate::identity::Id;
use crate::node;
use crate::parser::Parser;
use crate::service;
use crate::service::reactor::Io;
use crate::service::{filter, routing};
use crate::storage::refs::Refs;
use crate::storage::refs::SignedRefs;
use crate::storage::WriteStorage;
use crate::wire::transcoder::{DecodedData, Transcode};

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

impl<T> Decode for Vec<T>
where
    T: Decode,
{
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let len: Size = Size::decode(reader)?;
        let mut vec = Vec::with_capacity(len as usize);

        for _ in 0..len {
            let item = T::decode(reader)?;
            vec.push(item);
        }
        Ok(vec)
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

#[derive(Debug)]
pub struct Wire<R, S, W, G, T: Transcode> {
    inboxes: HashMap<net::SocketAddr, Parser>,
    inner: service::Service<R, S, W, G>,
    transcoder: T,
    handshake_queue: VecDeque<(net::SocketAddr, Vec<u8>)>,
}

impl<R, S, W, G, T: Transcode> Wire<R, S, W, G, T> {
    pub fn new(inner: service::Service<R, S, W, G>, transcoder: T) -> Self {
        Self {
            inboxes: HashMap::new(),
            inner,
            transcoder,
            handshake_queue: Default::default(),
        }
    }
}

impl<R, S, W, G, T> Wire<R, S, W, G, T>
where
    R: routing::Store,
    S: address::Store,
    W: WriteStorage + 'static,
    G: Signer,
    T: Transcode,
{
    pub fn connected(&mut self, addr: net::SocketAddr, local_addr: &net::SocketAddr, link: Link) {
        self.inboxes.insert(addr, Parser::new(256));
        self.inner.connected(addr, local_addr, link)
    }

    pub fn disconnected(
        &mut self,
        addr: &net::SocketAddr,
        reason: nakamoto::DisconnectReason<service::DisconnectReason>,
    ) {
        self.inboxes.remove(&addr);
        self.inner.disconnected(addr, &reason)
    }

    pub fn received_bytes(&mut self, addr: &net::SocketAddr, raw_bytes: &[u8]) {
        let bytes = match self.transcoder.input(raw_bytes) {
            // The received bytes were consumed by transcoder updating its inner
            // state (for instance, this happens during handshake).
            Ok(DecodedData::Local(bytes)) | Ok(DecodedData::Remote(bytes)) if bytes.is_empty() => {
                return
            }
            Ok(DecodedData::Local(bytes)) => bytes,
            Ok(DecodedData::Remote(output)) => {
                log::trace!("performing handshake, sending to the remote peer more data");
                self.handshake_queue.push_back((*addr, output));
                return;
            }
            Err(err) => {
                // TODO: Disconnect peer.
                log::error!("invalid transcoder input. Details: {}", err);
                return;
            }
        };

        if let Some(inbox) = self.inboxes.get_mut(addr) {
            inbox.input(&bytes);

            loop {
                match inbox.decode_next() {
                    Ok(Some(msg)) => self.inner.received_message(addr, msg),
                    Ok(None) => break,

                    Err(err) => {
                        // TODO: Disconnect peer.
                        log::error!("Invalid message received from {}: {}", addr, err);

                        return;
                    }
                }
            }
        } else {
            log::debug!("Received message from unknown peer {}", addr);
        }
    }
}

impl<R, S, W, G, T: Transcode> Iterator for Wire<R, S, W, G, T> {
    type Item = nakamoto::Io<service::Event, service::DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((addr, handshake_data)) = self.handshake_queue.pop_front() {
            return Some(nakamoto::Io::Write(addr, handshake_data));
        }

        match self.inner.next() {
            Some(Io::Write(addr, msgs)) => {
                let mut buf = Vec::new();
                for msg in msgs {
                    log::debug!("Write {:?} to {}", &msg, addr.ip());

                    msg.encode(&mut buf)
                        .expect("writing to an in-memory buffer doesn't fail");
                }
                Some(nakamoto::Io::Write(addr, buf))
            }
            Some(Io::Event(e)) => Some(nakamoto::Io::Event(e)),
            Some(Io::Connect(a)) => Some(nakamoto::Io::Connect(a)),
            Some(Io::Disconnect(a, r)) => Some(nakamoto::Io::Disconnect(a, r)),
            Some(Io::Wakeup(d)) => Some(nakamoto::Io::Wakeup(d)),

            None => None,
        }
    }
}

impl<R, S, W, G, T: Transcode> Deref for Wire<R, S, W, G, T> {
    type Target = service::Service<R, S, W, G>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<R, S, W, G, T: Transcode> DerefMut for Wire<R, S, W, G, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    use crate::crypto::Unverified;
    use crate::storage::refs::SignedRefs;
    use crate::test::{arbitrary, assert_matches};

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
    fn prop_string(input: String) -> quickcheck::TestResult {
        if input.len() > u8::MAX as usize {
            return quickcheck::TestResult::discard();
        }
        assert_eq!(deserialize::<String>(&serialize(&input)).unwrap(), input);

        quickcheck::TestResult::passed()
    }

    #[quickcheck]
    fn prop_vec(input: Vec<String>) {
        assert_eq!(
            deserialize::<Vec<String>>(&serialize(&input.as_slice())).unwrap(),
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
    fn prop_signature(input: arbitrary::ByteArray<64>) {
        let signature = Signature::from(input.into_inner());

        assert_eq!(
            deserialize::<Signature>(&serialize(&signature)).unwrap(),
            signature
        );
    }

    #[quickcheck]
    fn prop_oid(input: arbitrary::ByteArray<20>) {
        let oid = git::Oid::try_from(input.into_inner().as_slice()).unwrap();

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
}
