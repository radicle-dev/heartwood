//! Framing protocol.
#![warn(clippy::missing_docs_in_private_items)]
use std::{fmt, io};

use bytes::{Buf, BufMut};
use radicle::node::Link;

use crate::service::Message;
use crate::{PROTOCOL_VERSION, wire, wire::varint, wire::varint::VarInt};

/// Protocol version strings all start with the magic sequence `rad`, followed
/// by a version number.
pub const PROTOCOL_VERSION_STRING: Version = Version([b'r', b'a', b'd', PROTOCOL_VERSION]);

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
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_slice(&PROTOCOL_VERSION_STRING.0);
    }
}

impl wire::Decode for Version {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let mut version = [0u8; 4];

        buf.try_copy_to_slice(&mut version[..])?;

        if version != PROTOCOL_VERSION_STRING.0 {
            return Err(wire::Invalid::ProtocolVersion { actual: version }.into());
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
/// Additionally, Stream IDs must never be reused within a connection.
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
    pub fn kind(&self) -> Result<StreamType, u8> {
        let id = *self.0;
        let kind = ((id >> 1) & 0b11) as u8;

        StreamType::try_from(kind)
    }

    /// Create a control identifier.
    pub fn control(link: Link) -> Self {
        let link = if link.is_outbound() { 0 } else { 1 };
        Self(VarInt::from(((u8::from(StreamType::Control)) << 1) | link))
    }

    /// Create a gossip identifier.
    pub fn gossip(link: Link) -> Self {
        let link = if link.is_outbound() { 0 } else { 1 };
        Self(VarInt::from((u8::from(StreamType::Gossip) << 1) | link))
    }

    /// Create a git identifier.
    pub fn git(link: Link) -> Self {
        let link = if link.is_outbound() { 0 } else { 1 };
        Self(VarInt::from((u8::from(StreamType::Git) << 1) | link))
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
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let id = VarInt::decode(buf)?;
        Ok(Self(id))
    }
}

impl wire::Encode for StreamId {
    fn encode(&self, buf: &mut impl BufMut) {
        self.0.encode(buf)
    }
}

/// Type of stream.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StreamType {
    /// Control stream, used to open and close streams.
    Control = 0b00,
    /// Gossip stream, used to exchange messages.
    Gossip = 0b01,
    /// Git stream, used for replication.
    Git = 0b10,
}

impl TryFrom<u8> for StreamType {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(StreamType::Control),
            0b01 => Ok(StreamType::Gossip),
            0b10 => Ok(StreamType::Git),
            n => Err(n),
        }
    }
}

impl From<StreamType> for u8 {
    fn from(value: StreamType) -> Self {
        value as u8
    }
}

/// Protocol frame.
///
/// ```text
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |      'r'      |      'a'      |      'd'      |      0x1      | Version
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     Stream ID                           |TTT|I| Stream ID with Stream [T]ype and [I]nitiator bits
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     Data                                   ...| Data (variable size)
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
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
    /// Signal an end-of-file. This can be used to simulate connections terminating
    /// without having to close the connection. These control messages are turned into
    /// [`io::ErrorKind::UnexpectedEof`] errors on read.
    Eof {
        /// The stream to send an EOF on.
        stream: StreamId,
    },
}

/// Type of control message.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ControlType {
    /// Control open byte.
    Open = 0,
    /// Control close byte.
    Close = 1,
    /// Control EOF byte.
    Eof = 2,
}

impl TryFrom<u8> for ControlType {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(ControlType::Open),
            0b01 => Ok(ControlType::Close),
            0b10 => Ok(ControlType::Eof),
            n => Err(n),
        }
    }
}

impl From<ControlType> for u8 {
    fn from(value: ControlType) -> Self {
        value as u8
    }
}

impl wire::Decode for Control {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        match ControlType::try_from(u8::decode(buf)?) {
            Ok(ControlType::Open) => Ok(Control::Open {
                stream: StreamId::decode(buf)?,
            }),
            Ok(ControlType::Close) => Ok(Control::Close {
                stream: StreamId::decode(buf)?,
            }),
            Ok(ControlType::Eof) => Ok(Control::Eof {
                stream: StreamId::decode(buf)?,
            }),
            Err(other) => Err(wire::Invalid::ControlType { actual: other }.into()),
        }
    }
}

impl wire::Encode for Control {
    fn encode(&self, buf: &mut impl BufMut) {
        match self {
            Self::Open { stream: id } => {
                u8::from(ControlType::Open).encode(buf);
                id.encode(buf);
            }
            Self::Eof { stream: id } => {
                u8::from(ControlType::Eof).encode(buf);
                id.encode(buf);
            }
            Self::Close { stream: id } => {
                u8::from(ControlType::Close).encode(buf);
                id.encode(buf);
            }
        }
    }
}

impl<M: wire::Decode> wire::Decode for Frame<M> {
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let version = Version::decode(buf)?;
        if version.number() != PROTOCOL_VERSION {
            return Err(wire::Invalid::ProtocolVersionUnsupported {
                actual: version.number(),
            }
            .into());
        }
        let stream = StreamId::decode(buf)?;

        match stream.kind() {
            Ok(StreamType::Control) => {
                let ctrl = Control::decode(buf)?;
                let frame = Frame {
                    version,
                    stream,
                    data: FrameData::Control(ctrl),
                };
                Ok(frame)
            }
            Ok(StreamType::Gossip) => {
                let data = varint::payload::decode(buf)?;
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
            Ok(StreamType::Git) => {
                let data = varint::payload::decode(buf)?;
                Ok(Frame::git(stream, data))
            }
            Err(n) => Err(wire::Invalid::StreamType { actual: n }.into()),
        }
    }
}

impl<M: wire::Encode> wire::Encode for Frame<M> {
    fn encode(&self, buf: &mut impl BufMut) {
        self.version.encode(buf);
        self.stream.encode(buf);
        match &self.data {
            FrameData::Control(ctrl) => ctrl.encode(buf),
            FrameData::Git(data) => varint::payload::encode(data, buf),
            FrameData::Gossip(msg) => varint::payload::encode(&msg.encode_to_vec(), buf),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_stream_id() {
        assert_eq!(StreamId(VarInt(0b000)).kind().unwrap(), StreamType::Control);
        assert_eq!(StreamId(VarInt(0b010)).kind().unwrap(), StreamType::Gossip);
        assert_eq!(StreamId(VarInt(0b100)).kind().unwrap(), StreamType::Git);
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

    #[test]
    fn test_encode_git_large() {
        use wire::Encode as _;

        let size = u16::MAX as usize * 3;
        assert!(
            size > (wire::Size::MAX as usize * 2),
            "we want to test sizes that are way larger than any gossip message"
        );

        let a_lot_of_data = vec![0u8; size];

        let frame: Frame<Message> = Frame::git(StreamId(0u8.into()), a_lot_of_data);

        // In previous versions since 3c5668e this would panic.
        let bytes = frame.encode_to_vec();

        assert!(
            bytes.len() > wire::Size::MAX as usize * 2,
            "just making sure that whatever was encoded is still quite large"
        );
    }
}
