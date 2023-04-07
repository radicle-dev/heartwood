//! Framing protocol.
#![warn(clippy::missing_docs_in_private_items)]
use std::{fmt, io};

use crate::{wire, wire::varint, wire::varint::VarInt, wire::Message, Link};

/// Protocol version strings all start with the magic sequence `rad`, followed
/// by a version number.
pub const PROTOCOL_VERSION: Version = Version([b'r', b'a', b'd', 0x1]);

/// Control open byte.
const CONTROL_OPEN: u8 = 0;
/// Control close byte.
const CONTROL_CLOSE: u8 = 1;
/// Control EOF byte.
const CONTROL_EOF: u8 = 2;

/// Protocol version.
pub struct Version([u8; 4]);

impl wire::Encode for Version {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&PROTOCOL_VERSION.0)?;

        Ok(PROTOCOL_VERSION.0.len())
    }
}

impl wire::Decode for Version {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let mut version = [0u8; 4];
        reader.read_exact(&mut version[..])?;

        if version != PROTOCOL_VERSION.0 {
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
        match (id >> 1) & 0b11 {
            0 => Ok(StreamKind::Control),
            1 => Ok(StreamKind::Gossip),
            2 => Ok(StreamKind::Git),
            n => Err(n as u8),
        }
    }

    /// Create a control identifier.
    pub fn control(link: Link) -> Self {
        match link {
            Link::Outbound => Self(VarInt::from(0b000u8)),
            Link::Inbound => Self(VarInt::from(0b001u8)),
        }
    }

    /// Create a gossip identifier.
    pub fn gossip(link: Link) -> Self {
        match link {
            Link::Outbound => Self(VarInt::from(0b010u8)),
            Link::Inbound => Self(VarInt::from(0b011u8)),
        }
    }

    /// Create a git identifier.
    pub fn git(link: Link) -> Self {
        match link {
            Link::Outbound => Self(VarInt::from(0b100u8)),
            Link::Inbound => Self(VarInt::from(0b101u8)),
        }
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
pub enum StreamKind {
    /// Control stream, used to open and close streams.
    Control,
    /// Gossip stream, used to exchange messages.
    Gossip,
    /// Git stream, used for replication.
    Git,
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
pub struct Frame {
    /// The protocol version.
    pub version: Version,
    /// The stream identifier.
    pub stream: StreamId,
    /// The frame payload.
    pub data: FrameData,
}

impl Frame {
    /// Create a 'git' protocol frame.
    pub fn git(stream: StreamId, data: Vec<u8>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            stream,
            data: FrameData::Git(data),
        }
    }

    /// Create a 'control' protocol frame.
    pub fn control(link: Link, ctrl: Control) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            stream: StreamId::control(link),
            data: FrameData::Control(ctrl),
        }
    }

    /// Create a 'gossip' protocol frame.
    pub fn gossip(link: Link, msg: Message) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            stream: StreamId::gossip(link),
            data: FrameData::Gossip(msg),
        }
    }

    /// Serialize frame to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        wire::serialize(self)
    }
}

/// Frame payload.
pub enum FrameData {
    /// Control frame payload.
    Control(Control),
    /// Gossip frame payload.
    Gossip(Message),
    /// Git frame payload. May contain packet-lines as well as packfile data.
    Git(Vec<u8>),
}

/// A control message sent over a control stream.
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

impl wire::Decode for Control {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let command = u8::decode(reader)?;
        match command {
            CONTROL_OPEN => {
                let stream = StreamId::decode(reader)?;
                Ok(Control::Open { stream })
            }
            CONTROL_CLOSE => {
                let stream = StreamId::decode(reader)?;
                Ok(Control::Close { stream })
            }
            CONTROL_EOF => {
                let stream = StreamId::decode(reader)?;
                Ok(Control::Eof { stream })
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
            Self::Eof { stream: id } => {
                n += CONTROL_EOF.encode(writer)?;
                n += id.encode(writer)?;
            }
            Self::Close { stream: id } => {
                n += CONTROL_CLOSE.encode(writer)?;
                n += id.encode(writer)?;
            }
        }
        Ok(n)
    }
}

impl wire::Decode for Frame {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, wire::Error> {
        let version = Version::decode(reader)?;
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
                let msg = Message::decode(reader)?;
                let frame = Frame {
                    version,
                    stream,
                    data: FrameData::Gossip(msg),
                };
                Ok(frame)
            }
            Ok(StreamKind::Git { .. }) => {
                let size = VarInt::decode(reader)?;
                let mut data = vec![0; *size as usize];
                reader.read_exact(&mut data[..])?;

                Ok(Frame::git(stream, data))
            }
            Err(n) => Err(wire::Error::InvalidStreamKind(n)),
        }
    }
}

impl wire::Encode for Frame {
    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut n = 0;

        n += self.version.encode(writer)?;
        n += self.stream.encode(writer)?;

        match &self.data {
            FrameData::Control(ctrl) => {
                n += ctrl.encode(writer)?;
            }
            FrameData::Gossip(msg) => {
                n += msg.encode(writer)?;
            }
            FrameData::Git(data) => {
                let len = data.len();
                let size = VarInt::new(len as u64)
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                n += size.encode(writer)?;

                writer.write_all(data.as_slice())?;
                n += len;
            }
        }
        Ok(n)
    }
}
