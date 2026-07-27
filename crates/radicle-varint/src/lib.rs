//! Variable-length integer implementation ased on the QUIC variable-length integer encoding ([RFC 9000, Sec. 16]):
//!
//! The main type of this crate is [`VarInt`].
//!  
//! [RFC 9000, Sec. 16]: https://datatracker.ietf.org/doc/html/rfc9000#name-variable-length-integer-enc

use std::{fmt, ops};

/// Error returned when constructing a [`VarInt`] from a value greater than or equal to 2^62.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct BoundsExceeded(u64);

impl std::fmt::Debug for BoundsExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is greater than maximum VarInt {}",
            self.0,
            VarInt::MAX.0
        )
    }
}

/// An integer less than 2^62.
///
/// Based on the QUIC variable-length integer encoding ([RFC 9000, Sec. 16]):
///
/// > The QUIC variable-length integer encoding reserves the two most significant bits of the first
/// > byte to encode the base-2 logarithm of the integer encoding length in bytes. The integer value is
/// > encoded on the remaining bits, in network byte order. This means that integers are encoded on 1,
/// > 2, 4, or 8 bytes and can encode 6-, 14-, 30-, or 62-bit values, respectively. [The following Table]
/// > summarizes the encoding properties.
/// >
/// > | 2MSB | Length | Usable Bits | Range                          |
/// > |------|-------:|------------:|-------------------------------:|
/// > |   00 |      1 |           6 |                        0 -- 63 |
/// > |   01 |      2 |          14 |                    0 -- 16 383 |
/// > |   10 |      4 |          30 |             0 -- 1 073 741 823 |
/// > |   11 |      8 |          62 | 0 -- 4 611 686 018 427 387 903 |
///
/// [RFC 9000, Sec. 16]: https://datatracker.ietf.org/doc/html/rfc9000#name-variable-length-integer-enc
#[derive(Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct VarInt(u64);

impl VarInt {
    /// The largest representable value, which is 2^62 - 1.
    pub const MAX: VarInt = VarInt((1 << 62) - 1);

    const _ASSERT_MAX_MATCHES_RFC_9000: () = assert!(Self::MAX.0 == 4_611_686_018_427_387_903);

    /// Succeeds if `x` < 2^62.
    pub fn new(x: u64) -> Result<Self, BoundsExceeded> {
        if x <= Self::MAX.0 {
            Ok(Self(x))
        } else {
            Err(BoundsExceeded(x))
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

enum Seal {}

/// An extension trait for [`bytes::Buf`] that provides [`try_get_uvar`]
/// in analogy to [`try_get_u8`], [`try_get_u16`], [`try_get_u32`],
/// [`try_get_u64`], etc.
///
/// This trait is method-sealed, see <https://predr.ag/blog/definitive-guide-to-sealed-traits-in-rust/>.
///
/// [`try_get_uvar`]: BufExt::try_get_uvar
/// [`try_get_u8`]: bytes::Buf::try_get_u8
/// [`try_get_u16`]: bytes::Buf::try_get_u16
/// [`try_get_u32`]: bytes::Buf::try_get_u32
/// [`try_get_u64`]: bytes::Buf::try_get_u64
pub trait BufExt: bytes::Buf {
    fn try_get_uvar(&mut self) -> Result<VarInt, bytes::TryGetError> {
        let mut tmp = [0; 8];
        tmp[0] = self.try_get_u8()?;

        // Length is obtained by shifting the first two bits to the right,
        // zeroing all remaining bits.
        let len = tmp[0] >> 6;

        debug_assert_eq!(len & 0b1111_1100, 0);

        tmp[0] &= 0b0011_1111;

        let n = match len {
            0b00 => u64::from(tmp[0]),
            0b01 => {
                self.try_copy_to_slice(&mut tmp[1..2])?;
                u64::from(u16::from_be_bytes([tmp[0], tmp[1]]))
            }
            0b10 => {
                self.try_copy_to_slice(&mut tmp[1..4])?;
                u64::from(u32::from_be_bytes([tmp[0], tmp[1], tmp[2], tmp[3]]))
            }
            0b11 => {
                self.try_copy_to_slice(&mut tmp[1..8])?;
                u64::from_be_bytes(tmp)
            }
            _ => unreachable!(
                r#"
              There are not other patterns for two bits.
              The remaining six bits of `len` were zeroed (by the right shift above).
            "#
            ),
        };

        Ok(VarInt(n))
    }

    #[allow(private_interfaces)]
    fn seal(_: Seal);
}

impl<T> BufExt for T
where
    T: bytes::Buf,
{
    #[allow(private_interfaces)]
    fn seal(_: Seal) {}
}

/// An extension trait for [`bytes::BufMut`] that provides [`put_uvar`]
/// in analogy to [`put_u8`], [`put_u16`], [`put_u32`], [`put_u64`], etc.
///
/// This trait is method-sealed, see <https://predr.ag/blog/definitive-guide-to-sealed-traits-in-rust/>.
///
/// [`put_uvar`]: BufMutExt::put_uvar
/// [`put_u8`]: bytes::BufMut::put_u8
/// [`put_u16`]: bytes::BufMut::put_u16
/// [`put_u32`]: bytes::BufMut::put_u32
/// [`put_u64`]: bytes::BufMut::put_u64
pub trait BufMutExt: bytes::BufMut {
    fn put_uvar(&mut self, n: VarInt) {
        const BITS: [u32; 4] = [6, 14, 30, 62];

        // `const BOUNDS: [u64; 4] = …` might be nicer, but we cannot use that
        // in patterns for the `match` below, so we define four constants.
        const BOUND_0: u64 = 1u64 << BITS[0];
        const BOUND_1: u64 = 1u64 << BITS[1];
        const BOUND_2: u64 = 1u64 << BITS[2];
        const BOUND_3: u64 = 1u64 << BITS[3];

        match n.0 {
            u64::MIN..BOUND_0 => self.put_u8(u8::try_from(n.0).expect("n fits u8")),
            BOUND_0..BOUND_1 => {
                const LEN: u16 = 1;
                self.put_u16((LEN << BITS[1]) | u16::try_from(n.0).expect("n fits u16"))
            }
            BOUND_1..BOUND_2 => {
                const LEN: u32 = 2;
                self.put_u32((LEN << BITS[2]) | u32::try_from(n.0).expect("n fits u32"))
            }
            BOUND_2..BOUND_3 => {
                const LEN: u64 = 3;
                self.put_u64((LEN << BITS[3]) | n.0)
            }
            BOUND_3..=u64::MAX => {
                unreachable!()
            }
        }
    }

    #[allow(private_interfaces)]
    fn seal(_: Seal);
}

impl<T> BufMutExt for T
where
    T: bytes::BufMut,
{
    #[allow(private_interfaces)]
    fn seal(_: Seal) {}
}

#[test]
#[should_panic(expected = "overflow")]
fn overflow() {
    let mut buf = Vec::new();
    buf.put_uvar(VarInt(u64::MAX));
}

#[test]
fn weird() {
    let buf: [u8; 2] = [0x40, 0x25];
    assert_eq!(buf.as_slice().try_get_uvar().unwrap(), VarInt(37));
}

#[test]
fn cases() {
    #[rustfmt::skip]
    const CASES: &[(u64, &[u8])] = &[
        // 2^0 bytes
        ( 0, &[ 0]),
        ( 1, &[ 1]),
        (10, &[10]),
        (37, &[37]),

        // 2^1 bytes
        (15_293, &[0x7b, 0xbd]),

        // 2^2 bytes
        (494_878_333, &[0x9d, 0x7f, 0x3e, 0x7d]),

        // 2^4 bytes
        (151_288_809_941_952_652, &[0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c]),
        (         10_000_000_000, &[0xc0, 0x00, 0x00, 0x02, 0x54, 0x0b, 0xe4, 0x00]),
    ];

    for (x, expected) in CASES {
        let n = VarInt(*x);
        let mut buf = Vec::new();
        buf.put_uvar(n);
        assert_eq!(buf, expected.to_vec());
        let decoded = buf.as_slice().try_get_uvar().unwrap();
        assert_eq!(n, decoded);
    }
}
