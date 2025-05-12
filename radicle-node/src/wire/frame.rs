//! Framing protocol.
#![warn(clippy::missing_docs_in_private_items)]
use std::{fmt, io};

use crate::{wire, wire::varint, wire::varint::VarInt, wire::Message, Link, PROTOCOL_VERSION};

use super::error::StreamError;

/// Protocol version strings all start with the magic sequence `rad`, followed
/// by a version number.
pub const PROTOCOL_VERSION_STRING: Version = Version([b'r', b'a', b'd', PROTOCOL_VERSION]);

/// Control open byte.
const CONTROL_OPEN: u8 = 0;
/// Control close byte.
const CONTROL_CLOSE: u8 = 1;
/// Control EOF byte.
/// This is here for backwards compatibility. Previous versions of would send
/// it to signal closing a particular stream. We now use [`CONTROL_CLOSE`]
/// instead. Errors, including unexpected end of file, are signaled via
/// [`CONTROL_ERROR`].
const CONTROL_EOF: u8 = 2;
/// Control error byte.
const CONTROL_ERROR: u8 = 3;

/// Protocol version.
#[derive(Debug, PartialEq, Eq)]
pub struct Version([u8; 4]);

impl Version {
    /// Version number.
    pub fn number(&self) -> u8 {
        self.0[3]
    }
}

impl wire::Encode for Version {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&PROTOCOL_VERSION_STRING.0)?;

        Ok(PROTOCOL_VERSION_STRING.0.len())
    }
}

impl wire::Decode for Version {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let mut version = [0u8; 4];
        reader.read_exact(&mut version[..])?;

        if version != PROTOCOL_VERSION_STRING.0 {
            return Err(wire::Error::InvalidProtocolVersion(version));
        }
        Ok(Self(version))
    }
}

/// Identifies a (multiplexed) stream.
///
/// Stream IDs are variable-length integers with the least significant 3 bits
/// denoting the stream type and initiator.
///
/// The first bit denotes the initiator (outbound or inbound), while the second
/// and third bit denote the stream type. See `StreamKind`.
///
/// In a situation where Alice connects to Bob, Alice will have the initiator
/// bit set to `1` for all streams she creates, while Bob will have it set to `0`.
///
/// This ensures that Stream IDs never collide.
/// Additionally, Stream IDs must never be re-used within a connection.
///
/// +=======+==================================+
/// | Bits  | Stream Type                      |
/// +=======+==================================+
/// | 0b000 | Outbound Control stream          |
/// +-------+----------------------------------+
/// | 0b001 | Inbound Control stream           |
/// +-------+----------------------------------+
/// | 0b010 | Outbound Gossip stream           |
/// +-------+----------------------------------+
/// | 0b011 | Inbound Gossip stream            |
/// +-------+----------------------------------+
/// | 0b100 | Outbound Git stream              |
/// +-------+----------------------------------+
/// | 0b101 | Inbound Git stream               |
/// +-------+----------------------------------+
///
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StreamId(VarInt);

impl StreamId {
    /// Get the initiator of this stream.
    pub fn link(&self) -> Link {
        let n = *self.0;
        if 0b1 & n == 0 {
            Link::Outbound
        } else {
            Link::Inbound
        }
    }

    /// Get the kind of stream this is.
    pub fn kind(&self) -> Result<StreamKind, u8> {
        let id = *self.0;
        let kind = ((id >> 1) & 0b11) as u8;

        StreamKind::try_from(kind)
    }

    /// Create a control identifier.
    pub fn control(link: Link) -> Self {
        let link = if link.is_outbound() { 0 } else { 1 };
        Self(VarInt::from(((StreamKind::Control as u8) << 1) | link))
    }

    /// Create a gossip identifier.
    pub fn gossip(link: Link) -> Self {
        let link = if link.is_outbound() { 0 } else { 1 };
        Self(VarInt::from(((StreamKind::Gossip as u8) << 1) | link))
    }

    /// Create a git identifier.
    pub fn git(link: Link) -> Self {
        let link = if link.is_outbound() { 0 } else { 1 };
        Self(VarInt::from(((StreamKind::Git as u8) << 1) | link))
    }

    /// Get the nth identifier while preserving the stream type and initiator.
    pub fn nth(self, n: u64) -> Result<Self, varint::BoundsExceeded> {
        let id = *self.0 + (n << 3);
        VarInt::new(id).map(Self)
    }
}

impl From<StreamId> for u64 {
    fn from(value: StreamId) -> Self {
        *value.0
    }
}

impl From<StreamId> for VarInt {
    fn from(value: StreamId) -> Self {
        value.0
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self.0)
    }
}

impl wire::Decode for StreamId {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let id = VarInt::decode(reader)?;
        Ok(Self(id))
    }
}

impl wire::Encode for StreamId {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.0.encode(writer)
    }
}

/// Type of stream.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StreamKind {
    /// Control stream, used to open and close streams.
    Control = 0b00,
    /// Gossip stream, used to exchange messages.
    Gossip = 0b01,
    /// Git stream, used for replication.
    Git = 0b10,
}

impl TryFrom<u8> for StreamKind {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(StreamKind::Control),
            0b01 => Ok(StreamKind::Gossip),
            0b10 => Ok(StreamKind::Git),
            n => Err(n),
        }
    }
}

/// Protocol frame.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |      'r'      |      'a'      |      'd'      |      0x1      | Version
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     Stream ID                           |TTT|I| Stream ID with Stream [T]ype and [I]nitiator bits
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     Data                                   ...| Data (variable size)
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, PartialEq, Eq)]
pub struct Frame<M = Message> {
    /// The protocol version.
    pub version: Version,
    /// The stream identifier.
    pub stream: StreamId,
    /// The frame payload.
    pub data: FrameData<M>,
}

impl<M> Frame<M> {
    /// Create a 'git' protocol frame.
    pub fn git(stream: StreamId, data: Vec<u8>) -> Self {
        Self {
            version: PROTOCOL_VERSION_STRING,
            stream,
            data: FrameData::Git(data),
        }
    }

    /// Create a 'control' protocol frame.
    pub fn control(link: Link, ctrl: Control) -> Self {
        Self {
            version: PROTOCOL_VERSION_STRING,
            stream: StreamId::control(link),
            data: FrameData::Control(ctrl),
        }
    }

    /// Create a 'gossip' protocol frame.
    pub fn gossip(link: Link, msg: M) -> Self {
        Self {
            version: PROTOCOL_VERSION_STRING,
            stream: StreamId::gossip(link),
            data: FrameData::Gossip(msg),
        }
    }
}

impl<M: wire::Encode> Frame<M> {
    /// Serialize frame to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        wire::serialize(self)
    }
}

/// Frame payload.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameData<M> {
    /// Control frame payload.
    Control(Control),
    /// Gossip frame payload.
    Gossip(M),
    /// Git frame payload. May contain packet-lines as well as packfile data.
    Git(Vec<u8>),
}

/// A control message sent over a control stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Control {
    /// Open a new stream.
    Open {
        /// The stream to open.
        stream: StreamId,
    },
    /// Close an existing stream.
    Close {
        /// The stream to close.
        stream: StreamId,
    },
    /// Signal that an error happened on a stream, which left it in an
    /// unrecoverable state.
    Error {
        /// The stream to report an error for.
        stream: StreamId,
        /// What happened to that stream.
        error: StreamError,
    },
}

impl wire::Decode for Control {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let command = u8::decode(reader)?;
        match command {
            CONTROL_OPEN => {
                let stream = StreamId::decode(reader)?;
                Ok(Control::Open { stream })
            }
            CONTROL_CLOSE | CONTROL_EOF => {
                let stream = StreamId::decode(reader)?;
                Ok(Control::Close { stream })
            }
            CONTROL_ERROR => {
                let stream = StreamId::decode(reader)?;
                let error = StreamError::decode(reader)?;
                Ok(Control::Error { stream, error })
            }
            other => Err(wire::Error::InvalidControlMessage(other)),
        }
    }
}

impl wire::Encode for Control {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        match self {
            Self::Open { stream: id } => {
                n += CONTROL_OPEN.encode(writer)?;
                n += id.encode(writer)?;
            }
            Self::Close { stream: id } => {
                n += CONTROL_CLOSE.encode(writer)?;
                n += id.encode(writer)?;
            }
            Self::Error { stream: id, error } => {
                n += CONTROL_ERROR.encode(writer)?;
                n += id.encode(writer)?;
                n += error.encode(writer)?;
            }
        }
        Ok(n)
    }
}

impl<M: wire::Decode> wire::Decode for Frame<M> {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let version = Version::decode(reader)?;
        if version.number() != PROTOCOL_VERSION {
            return Err(wire::Error::WrongProtocolVersion(version.number()));
        }
        let stream = StreamId::decode(reader)?;

        match stream.kind() {
            Ok(StreamKind::Control) => {
                let ctrl = Control::decode(reader)?;
                let frame = Frame {
                    version,
                    stream,
                    data: FrameData::Control(ctrl),
                };
                Ok(frame)
            }
            Ok(StreamKind::Gossip) => {
                let data = varint::payload::decode(reader)?;
                let mut cursor = io::Cursor::new(data);
                let msg = M::decode(&mut cursor)?;
                let frame = Frame {
                    version,
                    stream,
                    data: FrameData::Gossip(msg),
                };

                // Nb. If there is data after the `Message` that is not decoded,
                // it is simply dropped here.

                Ok(frame)
            }
            Ok(StreamKind::Git { .. }) => {
                let data = varint::payload::decode(reader)?;
                Ok(Frame::git(stream, data))
            }
            Err(n) => Err(wire::Error::InvalidStreamKind(n)),
        }
    }
}

impl<M: wire::Encode> wire::Encode for Frame<M> {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.version.encode(writer)?;
        n += self.stream.encode(writer)?;
        n += match &self.data {
            FrameData::Control(ctrl) => ctrl.encode(writer)?,
            FrameData::Git(data) => varint::payload::encode(data, writer)?,
            FrameData::Gossip(msg) => varint::payload::encode(&wire::serialize(msg), writer)?,
        };

        Ok(n)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_stream_id() {
        assert_eq!(StreamId(VarInt(0b000)).kind().unwrap(), StreamKind::Control);
        assert_eq!(StreamId(VarInt(0b010)).kind().unwrap(), StreamKind::Gossip);
        assert_eq!(StreamId(VarInt(0b100)).kind().unwrap(), StreamKind::Git);
        assert_eq!(StreamId(VarInt(0b001)).link(), Link::Inbound);
        assert_eq!(StreamId(VarInt(0b000)).link(), Link::Outbound);
        assert_eq!(StreamId(VarInt(0b101)).link(), Link::Inbound);
        assert_eq!(StreamId(VarInt(0b100)).link(), Link::Outbound);

        assert_eq!(StreamId::git(Link::Outbound), StreamId(VarInt(0b100)));
        assert_eq!(StreamId::control(Link::Outbound), StreamId(VarInt(0b000)));
        assert_eq!(StreamId::gossip(Link::Outbound), StreamId(VarInt(0b010)));

        assert_eq!(StreamId::git(Link::Inbound), StreamId(VarInt(0b101)));
        assert_eq!(StreamId::control(Link::Inbound), StreamId(VarInt(0b001)));
        assert_eq!(StreamId::gossip(Link::Inbound), StreamId(VarInt(0b011)));
    }
}
