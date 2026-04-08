//! Variable-length integer implementation based on QUIC.
#![warn(clippy::missing_docs_in_private_items)]

// This implementation is largely based on the `quinn` crate.
// Copyright (c) 2018 The quinn developers.
use std::{fmt, ops};

use bytes::{Buf, BufMut};
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

    pub fn new_unchecked(x: u64) -> Self {
        Self(x)
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
    fn decode(buf: &mut impl Buf) -> Result<Self, wire::Error> {
        let mut tmp = [0; 8];
        tmp[0] = buf.try_get_u8()?;

        // Integer length.
        let tag = tmp[0] >> 6;
        tmp[0] &= 0b0011_1111;

        let x = match tag {
            0b00 => u64::from(tmp[0]),
            0b01 => {
                buf.try_copy_to_slice(&mut tmp[1..2])?;
                u64::from(u16::from_be_bytes([tmp[0], tmp[1]]))
            }
            0b10 => {
                buf.try_copy_to_slice(&mut tmp[1..4])?;
                u64::from(u32::from_be_bytes([tmp[0], tmp[1], tmp[2], tmp[3]]))
            }
            0b11 => {
                buf.try_copy_to_slice(&mut tmp[1..8])?;
                u64::from_be_bytes(tmp)
            }
            // SAFETY: It should be obvious that we can't have any other bit pattern
            // than the above, since all other bits are zeroed.
            _ => unreachable! {},
        };
        Ok(Self(x))
    }
}

impl Encode for VarInt {
    fn encode(&self, w: &mut impl BufMut) {
        let x: u64 = self.0;

        if x < 2u64.pow(6) {
            (x as u8).encode(w)
        } else if x < 2u64.pow(14) {
            ((0b01 << 14) | x as u16).encode(w)
        } else if x < 2u64.pow(30) {
            ((0b10 << 30) | x as u32).encode(w)
        } else if x < 2u64.pow(62) {
            ((0b11 << 62) | x).encode(w)
        } else {
            panic!("VarInt::encode: integer overflow");
        }
    }
}

/// Encoding and decoding varint-prefixed payloads.
pub mod payload {
    use super::*;

    /// Encode varint-prefixed data payload.
    pub fn encode(payload: &[u8], buf: &mut impl BufMut) {
        let len = payload.len();
        let varint = VarInt::new_unchecked(len as u64);

        varint.encode(buf); // The length of the payload length.
        buf.put_slice(payload);
    }

    /// Decode varint-prefixed data payload.
    pub fn decode(buf: &mut impl Buf) -> Result<Vec<u8>, wire::Error> {
        let size = VarInt::decode(buf)?;
        let mut data = vec![0; *size as usize];
        buf.try_copy_to_slice(&mut data[..])?;

        Ok(data)
    }
}

#[cfg(test)]
mod test {
    use qcheck_macros::quickcheck;

    use crate::prop_roundtrip;

    use super::*;

    prop_roundtrip!(VarInt);

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

    #[test]
    #[should_panic(expected = "overflow")]
    fn test_encode_overflow() {
        VarInt(u64::MAX).encode_to_vec();
    }

    #[test]
    fn test_encoding() {
        assert_eq!(VarInt(0).encode_to_vec(), vec![0x0]);
        assert_eq!(VarInt(1).encode_to_vec(), vec![0x01]);
        assert_eq!(VarInt(10).encode_to_vec(), vec![0x0a]);
        assert_eq!(VarInt(37).encode_to_vec(), vec![0x25]);
        assert_eq!(VarInt::decode_exact(&[0x40, 0x25]).unwrap(), VarInt(37));
        assert_eq!(VarInt(15293).encode_to_vec(), vec![0x7b, 0xbd]);
        assert_eq!(
            VarInt(494878333).encode_to_vec(),
            vec![0x9d, 0x7f, 0x3e, 0x7d],
        );
        assert_eq!(
            VarInt(151288809941952652).encode_to_vec(),
            vec![0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c]
        );
        assert_eq!(
            VarInt(10000000000).encode_to_vec(),
            vec![0xc0, 0x00, 0x00, 0x02, 0x54, 0x0b, 0xe4, 0x00],
        );
    }
}
