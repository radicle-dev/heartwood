pub mod message;

use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::net::IpAddr;
use std::ops::{Deref, DerefMut};
use std::string::FromUtf8Error;
use std::{io, mem};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use nakamoto_net as nakamoto;
use nakamoto_net::Link;

use crate::address_book;
use crate::crypto::{PublicKey, Signature, Signer, Unverified};
use crate::decoder::Decoder;
use crate::git;
use crate::git::fmt;
use crate::hash::Digest;
use crate::identity::Id;
use crate::service;
use crate::service::filter;
use crate::service::reactor::Io;
use crate::storage::refs::Refs;
use crate::storage::refs::SignedRefs;
use crate::storage::WriteStorage;

/// The default type we use to represent sizes.
/// Four bytes is more than enough for anything sent over the wire.
/// Note that in certain cases, we may use only one or two byte types.
pub type Size = u32;

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
    #[error("invalid git url `{url}`: {error}")]
    InvalidGitUrl {
        url: String,
        error: git::url::parse::Error,
    },
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

impl Encode for usize {
    /// We encode this type to a [`u32`], since there's no need to send larger messages
    /// over the network.
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        assert!(
            *self <= u32::MAX as usize,
            "Cannot encode sizes larger than {}",
            u32::MAX
        );
        writer.write_u32::<NetworkEndian>(*self as u32)?;

        Ok(mem::size_of::<u32>())
    }
}

impl Encode for PublicKey {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.deref().encode(writer)
    }
}

impl<const T: usize> Encode for &[u8; T] {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
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
        let mut n = self.len().encode(writer)?;

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
        let len = usize::decode(reader)?;
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
        let len = usize::decode(reader)?;
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

impl Decode for usize {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let size: usize = u32::decode(reader)?
            .try_into()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;

        Ok(size)
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

impl Decode for git::Url {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let url = String::decode(reader)?;
        let url = Self::from_bytes(url.as_bytes())
            .map_err(|error| Error::InvalidGitUrl { url, error })?;

        Ok(url)
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
        let size: Size = Decode::decode(reader)?;
        if size as usize != filter::FILTER_SIZE {
            return Err(Error::InvalidFilterSize(size as usize));
        }
        let bytes: [u8; filter::FILTER_SIZE] = Decode::decode(reader)?;
        let bf = filter::BloomFilter::from(Vec::from(bytes));

        debug_assert_eq!(bf.hashes(), filter::FILTER_HASHES);

        Ok(Self::from(bf))
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

#[derive(Debug)]
pub struct Wire<S, T, G> {
    inboxes: HashMap<IpAddr, Decoder>,
    inner: service::Service<S, T, G>,
}

impl<S, T, G> Wire<S, T, G> {
    pub fn new(inner: service::Service<S, T, G>) -> Self {
        Self {
            inboxes: HashMap::new(),
            inner,
        }
    }
}

impl<S, T, G> Wire<S, T, G>
where
    S: address_book::Store,
    T: WriteStorage + 'static,
    G: Signer,
{
    pub fn connected(
        &mut self,
        addr: std::net::SocketAddr,
        local_addr: &std::net::SocketAddr,
        link: Link,
    ) {
        self.inboxes.insert(addr.ip(), Decoder::new(256));
        self.inner.connected(addr, local_addr, link)
    }

    pub fn disconnected(
        &mut self,
        addr: &std::net::SocketAddr,
        reason: nakamoto::DisconnectReason<service::DisconnectReason>,
    ) {
        self.inboxes.remove(&addr.ip());
        self.inner.disconnected(addr, reason)
    }

    pub fn received_bytes(&mut self, addr: &std::net::SocketAddr, bytes: &[u8]) {
        let peer_ip = addr.ip();

        if let Some(inbox) = self.inboxes.get_mut(&peer_ip) {
            inbox.input(bytes);

            loop {
                match inbox.decode_next() {
                    Ok(Some(msg)) => self.inner.received_message(addr, msg),
                    Ok(None) => break,

                    Err(err) => {
                        // TODO: Disconnect peer.
                        log::error!("Invalid message received from {}: {}", peer_ip, err);

                        return;
                    }
                }
            }
        } else {
            log::debug!("Received message from unknown peer {}", peer_ip);
        }
    }
}

impl<S, T, G> Iterator for Wire<S, T, G> {
    type Item = nakamoto::Io<service::Event, service::DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
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

impl<S, T, G> Deref for Wire<S, T, G> {
    type Target = service::Service<S, T, G>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<S, T, G> DerefMut for Wire<S, T, G> {
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
    use crate::test::arbitrary;

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
    fn prop_usize(input: usize) -> quickcheck::TestResult {
        if input > u32::MAX as usize {
            return quickcheck::TestResult::discard();
        }
        assert_eq!(deserialize::<usize>(&serialize(&input)).unwrap(), input);

        quickcheck::TestResult::passed()
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
    fn test_git_url() {
        let url = git::Url {
            scheme: git::url::Scheme::Https,
            path: "/git".to_owned().into(),
            host: Some("seed.radicle.xyz".to_owned()),
            port: Some(8888),
            ..git::Url::default()
        };
        assert_eq!(deserialize::<git::Url>(&serialize(&url)).unwrap(), url);
    }
}
