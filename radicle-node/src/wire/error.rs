use std::{fmt::Display, io::ErrorKind};

use super::varint::VarInt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StreamError {
    Unknown,
    Io(std::io::ErrorKind),
    Git,
}

impl Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "StreamError::Unknown"),
            Self::Io(kind) => write!(f, "StreamError::Io({})", kind),
            Self::Git => write!(f, "StreamError::Git"),
        }
    }
}

impl From<std::io::Error> for StreamError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.kind())
    }
}

impl super::Encode for std::io::ErrorKind {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        VarInt::from(match self {
            ErrorKind::Other => 0u8,
            ErrorKind::NotFound => 1,
            ErrorKind::PermissionDenied => 2,
            ErrorKind::ConnectionRefused => 3,
            ErrorKind::ConnectionReset => 4,
            ErrorKind::HostUnreachable => 5,
            ErrorKind::NetworkUnreachable => 6,
            ErrorKind::ConnectionAborted => 7,
            ErrorKind::NotConnected => 8,
            ErrorKind::AddrInUse => 9,
            ErrorKind::AddrNotAvailable => 10,
            ErrorKind::NetworkDown => 11,
            ErrorKind::BrokenPipe => 12,
            ErrorKind::AlreadyExists => 13,
            ErrorKind::WouldBlock => 14,
            ErrorKind::NotADirectory => 15,
            ErrorKind::IsADirectory => 16,
            ErrorKind::DirectoryNotEmpty => 17,
            ErrorKind::ReadOnlyFilesystem => 18,
            ErrorKind::StaleNetworkFileHandle => 19,
            ErrorKind::InvalidInput => 20,
            ErrorKind::InvalidData => 21,
            ErrorKind::TimedOut => 22,
            ErrorKind::WriteZero => 23,
            ErrorKind::StorageFull => 24,
            ErrorKind::NotSeekable => 25,
            ErrorKind::QuotaExceeded => 26,
            ErrorKind::FileTooLarge => 27,
            ErrorKind::ResourceBusy => 28,
            ErrorKind::ExecutableFileBusy => 29,
            ErrorKind::Deadlock => 30,
            ErrorKind::CrossesDevices => 31,
            ErrorKind::TooManyLinks => 32,
            ErrorKind::ArgumentListTooLong => 33,
            ErrorKind::Interrupted => 34,
            ErrorKind::Unsupported => 35,
            ErrorKind::UnexpectedEof => 36,
            ErrorKind::OutOfMemory => 37,
            unknown => {
                // We conflate the value of "other" and something that we
                // don't know yet. This is under the assumption that these
                // errors will be really niche. The warning below will
                // hopefully point our attention here if this ever becomes
                // a problem.
                log::warn!(target: "wire", "Encountered unknown error kind: {}", unknown);
                0
            }
        })
        .encode(writer)
    }
}

impl super::Encode for StreamError {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        Ok(match self {
            Self::Io(kind) => 1u8.encode(writer)? + kind.encode(writer)?,
            Self::Git => 2u8.encode(writer)?,
            Self::Unknown => 0u8.encode(writer)?,
        })
    }
}

impl super::Decode for ErrorKind {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, super::Error> {
        Ok(match u8::decode(reader)? {
            1 => ErrorKind::NotFound,
            2 => ErrorKind::PermissionDenied,
            3 => ErrorKind::ConnectionRefused,
            4 => ErrorKind::ConnectionReset,
            5 => ErrorKind::HostUnreachable,
            6 => ErrorKind::NetworkUnreachable,
            7 => ErrorKind::ConnectionAborted,
            8 => ErrorKind::NotConnected,
            9 => ErrorKind::AddrInUse,
            10 => ErrorKind::AddrNotAvailable,
            11 => ErrorKind::NetworkDown,
            12 => ErrorKind::BrokenPipe,
            13 => ErrorKind::AlreadyExists,
            14 => ErrorKind::WouldBlock,
            15 => ErrorKind::NotADirectory,
            16 => ErrorKind::IsADirectory,
            17 => ErrorKind::DirectoryNotEmpty,
            18 => ErrorKind::ReadOnlyFilesystem,
            19 => ErrorKind::StaleNetworkFileHandle,
            20 => ErrorKind::InvalidInput,
            21 => ErrorKind::InvalidData,
            22 => ErrorKind::TimedOut,
            23 => ErrorKind::WriteZero,
            24 => ErrorKind::StorageFull,
            25 => ErrorKind::NotSeekable,
            26 => ErrorKind::QuotaExceeded,
            27 => ErrorKind::FileTooLarge,
            28 => ErrorKind::ResourceBusy,
            29 => ErrorKind::ExecutableFileBusy,
            30 => ErrorKind::Deadlock,
            31 => ErrorKind::CrossesDevices,
            32 => ErrorKind::TooManyLinks,
            33 => ErrorKind::ArgumentListTooLong,
            34 => ErrorKind::Interrupted,
            35 => ErrorKind::Unsupported,
            36 => ErrorKind::UnexpectedEof,
            37 => ErrorKind::OutOfMemory,
            0 | 38u8..=u8::MAX => ErrorKind::Other,
        })
    }
}

impl super::Decode for StreamError {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, super::Error> {
        Ok(match u8::decode(reader)? {
            1 => Self::Io(ErrorKind::decode(reader)?),
            2 => Self::Git,
            _ => Self::Unknown,
        })
    }
}

impl<T: super::Encode> super::Encode for Option<T> {
    fn encode<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        Ok(match self {
            None => u8::encode(&0, writer)?,
            Some(value) => u8::encode(&1, writer)? + value.encode(writer)?,
        })
    }
}

impl<T: super::Decode> super::Decode for Option<T> {
    fn decode<R: std::io::Read + ?Sized>(reader: &mut R) -> Result<Self, super::Error> {
        match u8::decode(reader)? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(reader)?)),
            _ => Err(super::Error::UnexpectedBytes),
        }
    }
}
