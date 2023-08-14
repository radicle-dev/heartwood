//! Variable-length integer implementation based on QUIC.
#![warn(clippy::missing_docs_in_private_items)]

// This implementation is largely based on the `quinn` crate.
// Copyright (c) 2018 The quinn developers.
use std::{fmt, io, ops};

use byteorder::ReadBytesExt;
use thiserror::Error;

use crate::wire;
use crate::wire::{Decode, Encode};

/// An integer less than 2^62
///
/// Based on QUIC variable-length integers (RFC 9000).
///
/// > The QUIC variable-length integer encoding reserves the two most significant bits of the first
/// > byte to encode the base-2 logarithm of the integer encoding length in bytes. The integer value is
/// > encoded on the remaining bits, in network byte order. This means that integers are encoded on 1,
/// > 2, 4, or 8 bytes and can encode 6-, 14-, 30-, or 62-bit values, respectively. Table 4 summarizes
/// > the encoding properties.
///
/// ```text
/// MSB   Length   Usable Bits   Range
/// ----------------------------------------------------
/// 00    1        6             0 - 63
/// 01    2        14            0 - 16383
/// 10    4        30            0 - 1073741823
/// 11    8        62            0 - 4611686018427387903
/// ```
#[derive(Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct VarInt(pub(crate) u64);

impl VarInt {
    /// The largest representable value.
    pub const MAX: VarInt = VarInt((1 << 62) - 1);

    /// Succeeds iff `x` < 2^62.
    pub fn new(x: u64) -> Result<Self, BoundsExceeded> {
        if x <= Self::MAX.0 {
            Ok(Self(x))
        } else {
            Err(BoundsExceeded)
        }
    }
}

impl ops::Deref for VarInt {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u8> for VarInt {
    fn from(x: u8) -> Self {
        VarInt(x.into())
    }
}

impl From<u16> for VarInt {
    fn from(x: u16) -> Self {
        VarInt(x.into())
    }
}

impl From<u32> for VarInt {
    fn from(x: u32) -> Self {
        VarInt(x.into())
    }
}

impl std::convert::TryFrom<u64> for VarInt {
    type Error = BoundsExceeded;
    /// Succeeds iff `x` < 2^62.
    fn try_from(x: u64) -> Result<Self, BoundsExceeded> {
        VarInt::new(x)
    }
}

impl fmt::Debug for VarInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for VarInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when constructing a `VarInt` from a value >= 2^62.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Error)]
#[error("value too large for varint encoding")]
pub struct BoundsExceeded;

impl Decode for VarInt {
    fn decode<R: io::Read + ?Sized>(r: &mut R) -> Result<Self, wire::Error> {
        let mut buf = [0; 8];
        buf[0] = r.read_u8()?;

        // Integer length.
        let tag = buf[0] >> 6;
        buf[0] &= 0b0011_1111;

        let x = match tag {
            0b00 => u64::from(buf[0]),
            0b01 => {
                r.read_exact(&mut buf[1..2])?;
                u64::from(u16::from_be_bytes([buf[0], buf[1]]))
            }
            0b10 => {
                r.read_exact(&mut buf[1..4])?;
                u64::from(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
            }
            0b11 => {
                r.read_exact(&mut buf[1..8])?;
                u64::from_be_bytes(buf)
            }
            // SAFETY: It should be obvious that we can't have any other bit pattern
            // than the above, since all other bits are zeroed.
            _ => unreachable! {},
        };
        Ok(Self(x))
    }
}

impl Encode for VarInt {
    fn encode<W: io::Write + ?Sized>(&self, w: &mut W) -> io::Result<usize> {
        let x: u64 = self.0;

        if x < 2u64.pow(6) {
            (x as u8).encode(w)
        } else if x < 2u64.pow(14) {
            (0b01 << 14 | x as u16).encode(w)
        } else if x < 2u64.pow(30) {
            (0b10 << 30 | x as u32).encode(w)
        } else if x < 2u64.pow(62) {
            (0b11 << 62 | x).encode(w)
        } else {
            panic!("VarInt::encode: integer overflow");
        }
    }
}

/// Encoding and decoding varint-prefixed payloads.
pub mod payload {
    use super::*;

    /// Encode varint-prefixed data payload.
    pub fn encode<W: io::Write + ?Sized>(payload: &[u8], writer: &mut W) -> io::Result<usize> {
        let mut n = 0;
        let len = payload.len();
        let varint =
            VarInt::new(len as u64).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;

        n += varint.encode(writer)?; // The length of the payload length.
        n += len; // The length of the data payload itself.

        writer.write_all(payload)?;

        Ok(n)
    }

    /// Decode varint-prefixed data payload.
    pub fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Vec<u8>, wire::Error> {
        let size = VarInt::decode(reader)?;
        let mut data = vec![0; *size as usize];
        reader.read_exact(&mut data[..])?;

        Ok(data)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use qcheck_macros::quickcheck;

    impl qcheck::Arbitrary for VarInt {
        fn arbitrary(g: &mut qcheck::Gen) -> Self {
            let a = u16::arbitrary(g) as u64;
            let b = u32::arbitrary(g) as u64;
            let n = g
                .choose(&[
                    0,
                    1,
                    3,
                    7,
                    13,
                    37,
                    255,
                    4931,
                    54019,
                    69149,
                    151288809941952652,
                    u8::MAX as u64,
                    u16::MAX as u64,
                    u16::MAX as u64 - 1,
                    u32::MAX as u64,
                    u32::MAX as u64 - 1,
                    *Self::MAX,
                    a,
                    b,
                ])
                .copied()
                .unwrap();

            Self(n)
        }
    }

    #[quickcheck]
    fn prop_encode_decode(input: VarInt) {
        let encoded = wire::serialize(&input);
        let decoded: VarInt = wire::deserialize(&encoded).unwrap();

        assert_eq!(decoded, input);
    }

    #[test]
    #[should_panic]
    fn test_encode_overflow() {
        wire::serialize(&VarInt(u64::MAX));
    }

    #[test]
    fn test_encoding() {
        assert_eq!(wire::serialize(&VarInt(0)), vec![0x0]);
        assert_eq!(wire::serialize(&VarInt(1)), vec![0x01]);
        assert_eq!(wire::serialize(&VarInt(10)), vec![0x0a]);
        assert_eq!(wire::serialize(&VarInt(37)), vec![0x25]);
        assert_eq!(
            wire::deserialize::<VarInt>(&[0x40, 0x25]).unwrap(),
            VarInt(37)
        );
        assert_eq!(wire::serialize(&VarInt(15293)), vec![0x7b, 0xbd]);
        assert_eq!(
            wire::serialize(&VarInt(494878333)),
            vec![0x9d, 0x7f, 0x3e, 0x7d],
        );
        assert_eq!(
            wire::serialize(&VarInt(151288809941952652)),
            vec![0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c]
        );
        assert_eq!(
            wire::serialize(&VarInt(10000000000)),
            vec![0xc0, 0x00, 0x00, 0x02, 0x54, 0x0b, 0xe4, 0x00],
        );
    }
}
