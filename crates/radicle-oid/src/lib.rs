#![no_std]

//! This is a `no_std` crate which carries the struct [`Oid`] that represents
//! Git object identifiers. Currently, only SHA-1 digests are supported.
//!
//! # Feature Flags
//!
//! The default features are `sha1` and `std`.
//!
//! ## `sha1`
//!
//! Enabled by default, since SHA-1 is commonly used. Currently, this feature is
//! also *required* to build the crate. In the future, after support for other
//! hashes is added, it might become possible to build the crate without support
//! for SHA-1.
//!
//! ## `std`
//!
//! [`Hash`]: ::doc_std::hash::Hash
//!
//! Enabled by default, since it is expected that most dependents will use the
//! standard library.
//!
//! Provides an implementation of [`Hash`].
//!
//! ## `git2`
//!
//! [`git2::Oid`]: ::git2::Oid
//!
//! Provides conversions to/from [`git2::Oid`].
//!
//! Note that as of version 0.19.0,
//!
//! ## `gix`
//!
//! [`ObjectId`]: ::gix_hash::ObjectId
//!
//! Provides conversions to/from [`ObjectId`].
//!
//! ## `schemars`
//!
//! [`JsonSchema`]: ::schemars::JsonSchema
//!
//! Provides an implementation of [`JsonSchema`].
//!
//! ## `serde`
//!
//! [`Serialize`]: ::serde::ser::Serialize
//! [`Deserialize`]: ::serde::de::Deserialize
//!
//! Provides implementations of [`Serialize`] and [`Deserialize`].
//!
//! ## `qcheck`
//!
//! [`qcheck::Arbitrary`]: ::qcheck::Arbitrary
//!
//! Provides an implementation of [`qcheck::Arbitrary`].
//!
//! ## `radicle-git-ref-format`
//!
//! [`radicle_git_ref_format::Component`]: ::radicle_git_ref_format::Component
//! [`radicle_git_ref_format::RefString`]: ::radicle_git_ref_format::RefString
//!
//! Conversion to [`radicle_git_ref_format::Component`]
//! (and also [`radicle_git_ref_format::RefString`]).

#[cfg(doc)]
extern crate std as doc_std;

extern crate alloc;

// Remove this once other hashes (e.g., SHA-256, and potentially others)
// are supported, and this crate can build without [`Oid::Sha1`].
#[cfg(not(feature = "sha1"))]
compile_error!("The `sha1` feature is required.");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjectFormat {
    #[cfg(feature = "sha1")]
    Sha1 = 1,
    #[cfg(feature = "unstable-sha256")]
    Sha256 = 2,
}

#[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
#[non_exhaustive]
pub enum Oid {
    #[cfg(feature = "sha1")]
    Sha1([u8; Self::LEN_SHA1]),
    #[cfg(feature = "unstable-sha256")]
    Sha256([u8; Self::LEN_SHA256]),
}

/// Conversions to/from SHA-1.
// Note that we deliberately do not implement `From<[u8; 20]>` and `Into<[u8; 20]>`,
// for forwards compatibility: What if another hash with digests of the same
// length becomes popular?
#[cfg(feature = "sha1")]
impl Oid {
    /// The length of a SHA-1 object identifier in bytes.
    pub const LEN_SHA1: usize = 20;

    /// A SHA-1 object identifier with all digest bytes set to zero.
    /// This is sometimes used as a sentinel value to indicate the absence of
    /// an object.
    /// To compare whether an object identifier is zero, prefer the method
    /// [`Oid::is_zero`] over checking equality with this constant.
    pub const ZERO_SHA1: Self = Self::Sha1([0u8; Self::LEN_SHA1]);

    pub fn from_sha1(digest: [u8; Self::LEN_SHA1]) -> Self {
        Self::Sha1(digest)
    }

    pub fn into_sha1(&self) -> Option<[u8; Self::LEN_SHA1]> {
        match self {
            Oid::Sha1(digest) => Some(*digest),
            #[cfg(feature = "unstable-sha256")]
            _ => None,
        }
    }
}

#[cfg(feature = "unstable-sha256")]
/// Conversions to/from SHA-256.
impl Oid {
    /// The length of a SHA-256 object identifier in bytes.
    pub const LEN_SHA256: usize = 32;

    /// A SHA-256 object identifier with all digest bytes set to zero.
    /// This is sometimes used as a sentinel value to indicate the absence of
    /// an object.
    /// To compare whether an object identifier is zero, prefer the method
    /// [`Oid::is_zero`] over checking equality with this constant.
    pub const ZERO_SHA256: Self = Self::Sha256([0u8; Self::LEN_SHA256]);

    pub fn from_sha256(digest: [u8; Self::LEN_SHA256]) -> Self {
        Self::Sha256(digest)
    }

    pub fn into_sha256(&self) -> Option<[u8; Self::LEN_SHA256]> {
        match self {
            Oid::Sha256(digest) => Some(*digest),
            #[cfg(feature = "sha1")]
            _ => None,
        }
    }
}

/// Interaction with zero.
impl Oid {
    /// Test whether all bytes in this object identifier are zero.
    /// See also [`::git2::Oid::is_zero`].
    pub fn is_zero(&self) -> bool {
        <Self as AsRef<[u8]>>::as_ref(self).iter().all(|b| *b == 0)
    }

    pub fn zero(format: ObjectFormat) -> Self {
        match format {
            #[cfg(feature = "sha1")]
            ObjectFormat::Sha1 => Self::ZERO_SHA1,
            #[cfg(feature = "unstable-sha256")]
            ObjectFormat::Sha256 => Self::ZERO_SHA256,
        }
    }
}

impl Oid {
    pub fn object_format(&self) -> ObjectFormat {
        match self {
            #[cfg(feature = "sha1")]
            Oid::Sha1(_) => ObjectFormat::Sha1,
            #[cfg(feature = "unstable-sha256")]
            Oid::Sha256(_) => ObjectFormat::Sha256,
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            #[cfg(feature = "sha1")]
            Oid::Sha1(_) => Self::LEN_SHA1,
            #[cfg(feature = "unstable-sha256")]
            Oid::Sha256(_) => Self::LEN_SHA256,
        }
    }
}

impl AsRef<[u8]> for Oid {
    fn as_ref(&self) -> &[u8] {
        match self {
            #[cfg(feature = "sha1")]
            Oid::Sha1(array) => array,
            #[cfg(feature = "unstable-sha256")]
            Oid::Sha256(array) => array,
        }
    }
}

impl From<Oid> for alloc::boxed::Box<[u8]> {
    fn from(oid: Oid) -> Self {
        match oid {
            #[cfg(feature = "sha1")]
            Oid::Sha1(array) => alloc::boxed::Box::new(array),
            #[cfg(feature = "unstable-sha256")]
            Oid::Sha256(array) => alloc::boxed::Box::new(array),
        }
    }
}

pub mod str {
    use super::Oid;
    use core::str;

    /// Length of the string representation of a SHA-1 digest in hexadecimal notation.
    #[cfg(feature = "sha1")]
    pub(super) const SHA1_DIGEST_STR_LEN: usize = Oid::LEN_SHA1 * 2;

    /// Length of the string representation of a SHA-256 digest in hexadecimal notation.
    #[cfg(feature = "unstable-sha256")]
    pub(super) const SHA256_DIGEST_STR_LEN: usize = Oid::LEN_SHA256 * 2;

    impl str::FromStr for Oid {
        type Err = error::ParseOidError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            use error::ParseOidError::*;

            let len = s.len();

            #[cfg(feature = "sha1")]
            if len == SHA1_DIGEST_STR_LEN {
                let mut bytes = [0u8; Oid::LEN_SHA1];
                for i in 0..Oid::LEN_SHA1 {
                    bytes[i] = u8::from_str_radix(&s[i * 2..=i * 2 + 1], 16)
                        .map_err(|source| At { index: i, source })?;
                }

                return Ok(Self::Sha1(bytes));
            }

            #[cfg(feature = "unstable-sha256")]
            if len == SHA256_DIGEST_STR_LEN {
                let mut bytes = [0u8; Oid::LEN_SHA256];
                for i in 0..Oid::LEN_SHA256 {
                    bytes[i] = u8::from_str_radix(&s[i * 2..=i * 2 + 1], 16)
                        .map_err(|source| At { index: i, source })?;
                }

                return Ok(Self::Sha256(bytes));
            }

            Err(Len(len))
        }
    }

    pub mod error {
        use core::{fmt, num};

        #[cfg(feature = "sha1")]
        use super::SHA1_DIGEST_STR_LEN;

        #[cfg(feature = "unstable-sha256")]
        use super::SHA256_DIGEST_STR_LEN;

        pub enum ParseOidError {
            Len(usize),
            At {
                index: usize,
                source: num::ParseIntError,
            },
        }

        impl fmt::Display for ParseOidError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                use ParseOidError::*;
                match self {
                    #[cfg(all(feature = "sha1", not(feature = "unstable-sha256")))]
                    Len(len) => {
                        write!(f, "invalid length (have {len}, want {SHA1_DIGEST_STR_LEN})")
                    }
                    #[cfg(all(not(feature = "sha1"), feature = "unstable-sha256"))]
                    Len(len) => {
                        write!(
                            f,
                            "invalid length (have {len}, want {SHA256_DIGEST_STR_LEN})"
                        )
                    }
                    #[cfg(all(feature = "sha1", feature = "unstable-sha256"))]
                    Len(len) => {
                        write!(
                            f,
                            "invalid length (have {len}, want {SHA1_DIGEST_STR_LEN} or {SHA256_DIGEST_STR_LEN})"
                        )
                    }
                    At { index, source } => write!(
                        f,
                        "parse error at byte {index} (characters {} and {}): {source}",
                        index * 2,
                        index * 2 + 1
                    ),
                }
            }
        }

        impl fmt::Debug for ParseOidError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }

        impl core::error::Error for ParseOidError {
            fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
                match self {
                    ParseOidError::At { source, .. } => Some(source),
                    _ => None,
                }
            }
        }
    }

    pub use error::ParseOidError;

    #[cfg(test)]
    mod test {
        use super::*;
        use alloc::string::ToString;
        use qcheck_macros::quickcheck;

        #[cfg(feature = "sha1")]
        mod sha1 {
            use super::*;

            #[test]
            fn fixture() {
                assert_eq!(
                    "123456789abcdef0123456789abcdef012345678"
                        .parse::<Oid>()
                        .unwrap(),
                    Oid::from_sha1([
                        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                        0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                    ])
                );
            }

            #[test]
            fn zero() {
                assert_eq!(
                    "0000000000000000000000000000000000000000"
                        .parse::<Oid>()
                        .unwrap(),
                    Oid::ZERO_SHA1
                );
            }
        }

        #[cfg(feature = "unstable-sha256")]
        mod sha256 {
            use super::*;

            #[test]
            fn fixture() {
                assert_eq!(
                    "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0"
                        .parse::<Oid>()
                        .unwrap(),
                    Oid::from_sha256([
                        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                        0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                    ])
                );
            }

            #[test]
            fn zero() {
                assert_eq!(
                    "0000000000000000000000000000000000000000000000000000000000000000"
                        .parse::<Oid>()
                        .unwrap(),
                    Oid::ZERO_SHA256
                );
            }
        }

        #[quickcheck]
        fn git2_roundtrip(oid: Oid) {
            #[cfg(feature = "unstable-sha256")]
            if matches!(oid, Oid::Sha256(_)) {
                // `git2::Oid` does not support SHA-256, so skip this test for
                // SHA-256 object identifiers.
                return;
            }

            let other = git2::Oid::from(oid);
            let other = other.to_string();
            let other = other.parse::<Oid>().unwrap();
            assert_eq!(oid, other);
        }

        #[quickcheck]
        fn gix_roundtrip(oid: Oid) {
            let other = gix_hash::ObjectId::from(oid);
            let other = other.to_string();
            let other = other.parse::<Oid>().unwrap();
            assert_eq!(oid, other);
        }
    }
}

mod fmt {
    use alloc::format;
    use core::fmt;

    use super::Oid;

    impl fmt::Display for Oid {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                #[cfg(feature = "sha1")]
                Oid::Sha1(digest) =>
                // SAFETY (for all 20 blocks below): The length of `digest` is
                // known to be `SHA1_DIGEST_LEN`, which is 20.
                // The indices below are manually verified to not be out of bounds.
                format!(
                    "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    unsafe { digest.get_unchecked(0) },
                    unsafe { digest.get_unchecked(1) },
                    unsafe { digest.get_unchecked(2) },
                    unsafe { digest.get_unchecked(3) },
                    unsafe { digest.get_unchecked(4) },
                    unsafe { digest.get_unchecked(5) },
                    unsafe { digest.get_unchecked(6) },
                    unsafe { digest.get_unchecked(7) },
                    unsafe { digest.get_unchecked(8) },
                    unsafe { digest.get_unchecked(9) },
                    unsafe { digest.get_unchecked(10) },
                    unsafe { digest.get_unchecked(11) },
                    unsafe { digest.get_unchecked(12) },
                    unsafe { digest.get_unchecked(13) },
                    unsafe { digest.get_unchecked(14) },
                    unsafe { digest.get_unchecked(15) },
                    unsafe { digest.get_unchecked(16) },
                    unsafe { digest.get_unchecked(17) },
                    unsafe { digest.get_unchecked(18) },
                    unsafe { digest.get_unchecked(19) },
                ).fmt(f),
                #[cfg(feature = "unstable-sha256")]
                Oid::Sha256(digest) =>
                // SAFETY (for all 32 blocks below): The length of `digest` is
                // known to be `SHA256_DIGEST_LEN`, which is 32.
                // The indices below are manually verified to not be out of bounds.
                format!(
                    "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    unsafe { digest.get_unchecked(0) },
                    unsafe { digest.get_unchecked(1) },
                    unsafe { digest.get_unchecked(2) },
                    unsafe { digest.get_unchecked(3) },
                    unsafe { digest.get_unchecked(4) },
                    unsafe { digest.get_unchecked(5) },
                    unsafe { digest.get_unchecked(6) },
                    unsafe { digest.get_unchecked(7) },
                    unsafe { digest.get_unchecked(8) },
                    unsafe { digest.get_unchecked(9) },
                    unsafe { digest.get_unchecked(10) },
                    unsafe { digest.get_unchecked(11) },
                    unsafe { digest.get_unchecked(12) },
                    unsafe { digest.get_unchecked(13) },
                    unsafe { digest.get_unchecked(14) },
                    unsafe { digest.get_unchecked(15) },
                    unsafe { digest.get_unchecked(16) },
                    unsafe { digest.get_unchecked(17) },
                    unsafe { digest.get_unchecked(18) },
                    unsafe { digest.get_unchecked(19) },
                    unsafe { digest.get_unchecked(20) },
                    unsafe { digest.get_unchecked(21) },
                    unsafe { digest.get_unchecked(22) },
                    unsafe { digest.get_unchecked(23) },
                    unsafe { digest.get_unchecked(24) },
                    unsafe { digest.get_unchecked(25) },
                    unsafe { digest.get_unchecked(26) },
                    unsafe { digest.get_unchecked(27) },
                    unsafe { digest.get_unchecked(28) },
                    unsafe { digest.get_unchecked(29) },
                    unsafe { digest.get_unchecked(30) },
                    unsafe { digest.get_unchecked(31) },
                ).fmt(f),
            }
        }
    }

    impl fmt::Debug for Oid {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use alloc::string::ToString;
        use qcheck_macros::quickcheck;

        #[cfg(feature = "sha1")]
        mod sha1 {
            use super::*;

            #[test]
            fn fixture() {
                assert_eq!(
                    Oid::from_sha1([
                        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                        0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                    ])
                    .to_string(),
                    "123456789abcdef0123456789abcdef012345678"
                );
            }

            #[test]
            fn zero() {
                assert_eq!(
                    Oid::ZERO_SHA1.to_string(),
                    "0000000000000000000000000000000000000000"
                );
            }
        }

        #[cfg(feature = "unstable-sha256")]
        mod sha256 {
            use super::*;

            #[test]
            fn fixture() {
                assert_eq!(
                    Oid::from_sha256([
                        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                        0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0
                    ])
                    .to_string(),
                    "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0"
                );
            }

            #[test]
            fn zero() {
                assert_eq!(
                    Oid::ZERO_SHA256.to_string(),
                    "0000000000000000000000000000000000000000000000000000000000000000"
                );
            }
        }

        #[quickcheck]
        fn git2(oid: Oid) {
            #[cfg(feature = "unstable-sha256")]
            if matches!(oid, Oid::Sha256(_)) {
                // `git2::Oid` does not support SHA-256, so skip this test for
                // SHA-256 object identifiers.
                return;
            }

            assert_eq!(oid.to_string(), git2::Oid::from(oid).to_string());
        }

        #[quickcheck]
        fn gix(oid: Oid) {
            assert_eq!(oid.to_string(), gix_hash::ObjectId::from(oid).to_string());
        }
    }
}

#[cfg(feature = "std")]
mod std {
    extern crate std;

    use super::Oid;

    mod hash {
        use std::hash;

        use super::*;

        #[allow(clippy::derived_hash_with_manual_eq)]
        impl hash::Hash for Oid {
            fn hash<H: hash::Hasher>(&self, state: &mut H) {
                let bytes: &[u8] = self.as_ref();
                std::hash::Hash::hash(bytes, state)
            }
        }
    }
}

#[cfg(any(feature = "gix", test))]
mod gix {
    use gix_hash::ObjectId as Other;

    use super::ObjectFormat;
    use super::Oid;

    impl From<Other> for Oid {
        fn from(other: Other) -> Self {
            match other {
                #[cfg(feature = "sha1")]
                Other::Sha1(digest) => Self::Sha1(digest),
                #[cfg(feature = "unstable-sha256")]
                Other::Sha256(digest) => Self::Sha256(digest),
                _ => unimplemented!("conversion from {other:?} into radicle_oid::Oid"),
            }
        }
    }

    impl From<Oid> for Other {
        fn from(oid: Oid) -> Other {
            match oid {
                #[cfg(feature = "sha1")]
                Oid::Sha1(digest) => Other::Sha1(digest),
                #[cfg(feature = "unstable-sha256")]
                Oid::Sha256(digest) => Other::Sha256(digest),
            }
        }
    }

    impl core::cmp::PartialEq<Other> for Oid {
        fn eq(&self, other: &Other) -> bool {
            match (self, other) {
                #[cfg(feature = "sha1")]
                (Oid::Sha1(a), Other::Sha1(b)) => a == b,
                #[cfg(feature = "unstable-sha256")]
                (Oid::Sha256(a), Other::Sha256(b)) => a == b,
                #[cfg(all(feature = "sha1", feature = "unstable-sha256"))]
                (Oid::Sha1(_), Other::Sha256(_)) | (Oid::Sha256(_), Other::Sha1(_)) => false,
                _ => unimplemented!("conversion from {other:?} into radicle_oid::Oid"),
            }
        }
    }

    impl AsRef<gix_hash::oid> for Oid {
        fn as_ref(&self) -> &gix_hash::oid {
            match self {
                #[cfg(feature = "sha1")]
                Oid::Sha1(digest) => gix_hash::oid::from_bytes_unchecked(digest),
                #[cfg(feature = "unstable-sha256")]
                Oid::Sha256(digest) => gix_hash::oid::from_bytes_unchecked(digest),
            }
        }
    }

    impl From<gix_hash::Kind> for ObjectFormat {
        fn from(kind: gix_hash::Kind) -> Self {
            match kind {
                #[cfg(feature = "sha1")]
                gix_hash::Kind::Sha1 => Self::Sha1,
                #[cfg(feature = "unstable-sha256")]
                gix_hash::Kind::Sha256 => Self::Sha256,
                _ => unimplemented!("conversion from {kind:?} into radicle_oid::ObjectFormat"),
            }
        }
    }

    impl From<ObjectFormat> for gix_hash::Kind {
        fn from(format: ObjectFormat) -> Self {
            match format {
                #[cfg(feature = "sha1")]
                ObjectFormat::Sha1 => Self::Sha1,
                #[cfg(feature = "unstable-sha256")]
                ObjectFormat::Sha256 => Self::Sha256,
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use gix_hash::Kind;

        #[cfg(feature = "sha1")]
        mod sha1 {
            use super::*;

            #[test]
            fn zero() {
                let other = Other::null(Kind::Sha1);
                assert_eq!(Oid::ZERO_SHA1, other);
                assert_eq!(Oid::from(other), Oid::ZERO_SHA1);
            }
        }

        #[cfg(feature = "unstable-sha256")]
        mod sha256 {
            use super::*;

            #[test]
            fn zero() {
                let other = Other::null(Kind::Sha256);
                assert_eq!(Oid::ZERO_SHA256, other);
                assert_eq!(Oid::from(other), Oid::ZERO_SHA256);
            }
        }
    }
}

#[cfg(any(feature = "git2", test))]
mod git2 {
    use ::git2::Oid as Other;

    use super::*;

    #[cfg(feature = "sha1")]
    const EXPECT_SHA1: &str = "git2::Oid must be exactly 20 bytes long";

    #[cfg(feature = "unstable-sha256")]
    const EXPECT_SHA256: &str = "git2::Oid must be exactly 20 bytes long";

    #[cfg(feature = "sha1")]
    impl From<Other> for Oid {
        fn from(other: Other) -> Self {
            match other.object_format() {
                #[cfg(feature = "sha1")]
                ::git2::ObjectFormat::Sha1 => {
                    Self::Sha1(other.as_bytes().try_into().expect(EXPECT_SHA1))
                }
                #[cfg(feature = "unstable-sha256")]
                ::git2::ObjectFormat::Sha256 => {
                    Self::Sha256(other.as_bytes().try_into().expect(EXPECT_SHA256))
                }
                #[cfg(all(feature = "sha1", not(feature = "unstable-sha256")))]
                _ => {
                    unimplemented!(
                        "conversion from {other:?} into radicle_oid::Oid since object format is not equal to SHA-1",
                    )
                }
            }
        }
    }

    impl From<Oid> for Other {
        fn from(oid: Oid) -> Self {
            match oid {
                #[cfg(feature = "sha1")]
                Oid::Sha1(array) => Other::from_bytes(&array).expect(EXPECT_SHA1),
                #[cfg(feature = "unstable-sha256")]
                Oid::Sha256(array) => Other::from_bytes(&array).expect(EXPECT_SHA256),
            }
        }
    }

    impl From<&Oid> for Other {
        fn from(oid: &Oid) -> Self {
            match oid {
                #[cfg(feature = "sha1")]
                Oid::Sha1(array) => Other::from_bytes(array).expect(EXPECT_SHA1),
                #[cfg(feature = "unstable-sha256")]
                Oid::Sha256(array) => Other::from_bytes(array).expect(EXPECT_SHA256),
            }
        }
    }

    impl core::cmp::PartialEq<Other> for Oid {
        fn eq(&self, other: &Other) -> bool {
            other.as_bytes() == AsRef::<[u8]>::as_ref(&self)
        }
    }

    impl From<ObjectFormat> for ::git2::ObjectFormat {
        fn from(format: ObjectFormat) -> Self {
            match format {
                #[cfg(feature = "sha1")]
                ObjectFormat::Sha1 => Self::Sha1,
                #[cfg(feature = "unstable-sha256")]
                ObjectFormat::Sha256 => Self::Sha256,
            }
        }
    }

    impl From<::git2::ObjectFormat> for ObjectFormat {
        fn from(format: ::git2::ObjectFormat) -> Self {
            match format {
                #[cfg(feature = "sha1")]
                ::git2::ObjectFormat::Sha1 => Self::Sha1,
                #[cfg(feature = "unstable-sha256")]
                ::git2::ObjectFormat::Sha256 => Self::Sha256,
                #[cfg(all(feature = "sha1", not(feature = "unstable-sha256")))]
                _ => {
                    unimplemented!(
                        "conversion from {format:?} into radicle_oid::ObjectFormat since it is not equal to SHA-1",
                    )
                }
            }
        }
    }

    #[cfg(all(feature = "sha1", test))]
    mod test {
        use super::*;

        #[test]
        fn zero() {
            assert!(Oid::ZERO_SHA1 == Other::ZERO_SHA1);
        }
    }
}

#[cfg(any(test, feature = "qcheck"))]
mod test {
    mod qcheck {
        use ::qcheck::{Arbitrary, Gen};

        use crate::*;

        impl Arbitrary for Oid {
            #[cfg(all(feature = "sha1", not(feature = "unstable-sha256")))]
            fn arbitrary(g: &mut Gen) -> Self {
                Self::Sha1(<[u8; Oid::LEN_SHA1]>::arbitrary(g))
            }

            #[cfg(all(not(feature = "sha1"), feature = "unstable-sha256"))]
            fn arbitrary(g: &mut Gen) -> Self {
                Self::Sha256(<[u8; Oid::LEN_SHA256]>::arbitrary(g))
            }

            #[cfg(all(feature = "sha1", feature = "unstable-sha256"))]
            fn arbitrary(g: &mut Gen) -> Self {
                if bool::arbitrary(g) {
                    Self::Sha1(<[u8; Oid::LEN_SHA1]>::arbitrary(g))
                } else {
                    Self::Sha256(<[u8; Oid::LEN_SHA256]>::arbitrary(g))
                }
            }
        }

        impl Arbitrary for ObjectFormat {
            #[cfg(all(feature = "sha1", not(feature = "unstable-sha256")))]
            fn arbitrary(_g: &mut Gen) -> Self {
                Self::Sha1
            }

            #[cfg(all(not(feature = "sha1"), feature = "unstable-sha256"))]
            fn arbitrary(g: &mut Gen) -> Self {
                Self::Sha256
            }

            #[cfg(all(feature = "sha1", feature = "unstable-sha256"))]
            fn arbitrary(g: &mut Gen) -> Self {
                if bool::arbitrary(g) {
                    Self::Sha1
                } else {
                    Self::Sha256
                }
            }
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    mod ser {
        use ::serde::ser;

        use crate::*;

        impl ser::Serialize for Oid {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ser::Serializer,
            {
                serializer.collect_str(self)
            }
        }
    }

    mod de {
        use core::fmt;

        use ::serde::de;

        use crate::*;

        impl<'de> de::Deserialize<'de> for Oid {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                struct OidVisitor;

                impl<'de> de::Visitor<'de> for OidVisitor {
                    type Value = Oid;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(
                            f,
                            "a Git object identifier (SHA-1 digest in hexadecimal notation; {} characters; {} bytes)",
                            crate::str::SHA1_DIGEST_STR_LEN,
                            Oid::LEN_SHA1
                        )
                    }

                    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        s.parse().map_err(de::Error::custom)
                    }
                }

                deserializer.deserialize_str(OidVisitor)
            }
        }
    }
}

#[cfg(feature = "radicle-git-ref-format")]
mod radicle_git_ref_format {
    use ::radicle_git_ref_format::{Component, RefString};

    use super::*;

    impl From<&Oid> for Component<'_> {
        fn from(id: &Oid) -> Self {
            Component::from_refstr(RefString::from(id))
                .expect("Git object identifiers are valid component strings")
        }
    }

    impl From<&Oid> for RefString {
        fn from(id: &Oid) -> Self {
            RefString::try_from(alloc::format!("{id}"))
                .expect("Git object identifiers are valid reference strings")
        }
    }
}

#[cfg(feature = "schemars")]
mod schemars {
    use alloc::{borrow::Cow, format};

    use ::schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

    use super::Oid;

    impl JsonSchema for Oid {
        fn schema_name() -> Cow<'static, str> {
            "Oid".into()
        }

        fn schema_id() -> Cow<'static, str> {
            concat!(module_path!(), "::Oid").into()
        }

        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            use crate::str::SHA1_DIGEST_STR_LEN;
            json_schema!({
                "description": format!(
                    "A Git object identifier (SHA-1 digest in hexadecimal notation; {SHA1_DIGEST_STR_LEN} characters; {} bytes)",
                    Oid::LEN_SHA1,
                ),
                "type": "string",
                "maxLength": SHA1_DIGEST_STR_LEN,
                "minLength": SHA1_DIGEST_STR_LEN,
                "pattern":  format!("^[0-9a-fA-F]{{{SHA1_DIGEST_STR_LEN}}}$"),
            })
        }
    }
}
