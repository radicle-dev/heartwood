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

const SHA1_DIGEST_LEN: usize = 20;

#[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
#[non_exhaustive]
pub enum Oid {
    Sha1([u8; SHA1_DIGEST_LEN]),
}

/// Conversions to/from SHA-1.
// Note that we deliberately do not implement `From<[u8; 20]>` and `Into<[u8; 20]>`,
// for forwards compatibility: What if another hash with digests of the same
// length becomes popular?
impl Oid {
    pub fn from_sha1(digest: [u8; SHA1_DIGEST_LEN]) -> Self {
        Self::Sha1(digest)
    }

    pub fn into_sha1(&self) -> Option<[u8; SHA1_DIGEST_LEN]> {
        match self {
            Oid::Sha1(digest) => Some(*digest),
        }
    }

    pub fn sha1_zero() -> Self {
        Self::Sha1([0u8; SHA1_DIGEST_LEN])
    }
}

/// Interaction with zero.
impl Oid {
    /// Test whether all bytes in this object identifier are zero.
    /// See also [`::git2::Oid::is_zero`].
    pub fn is_zero(&self) -> bool {
        match self {
            Oid::Sha1(ref array) => array.iter().all(|b| *b == 0),
        }
    }
}

impl AsRef<[u8]> for Oid {
    fn as_ref(&self) -> &[u8] {
        match self {
            Oid::Sha1(ref array) => array,
        }
    }
}

impl From<Oid> for alloc::boxed::Box<[u8]> {
    fn from(oid: Oid) -> Self {
        match oid {
            Oid::Sha1(array) => alloc::boxed::Box::new(array),
        }
    }
}

pub mod str {
    use super::{Oid, SHA1_DIGEST_LEN};
    use core::str;

    /// Length of the string representation of a SHA-1 digest in hexadecimal notation.
    pub(super) const SHA1_DIGEST_STR_LEN: usize = SHA1_DIGEST_LEN * 2;

    impl str::FromStr for Oid {
        type Err = error::ParseOidError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            use error::ParseOidError::*;

            let len = s.len();
            if len != SHA1_DIGEST_STR_LEN {
                return Err(Len(len));
            }

            let mut bytes = [0u8; SHA1_DIGEST_LEN];
            for i in 0..SHA1_DIGEST_LEN {
                bytes[i] = u8::from_str_radix(&s[i * 2..=i * 2 + 1], 16)
                    .map_err(|source| At { index: i, source })?;
            }

            Ok(Self::Sha1(bytes))
        }
    }

    pub mod error {
        use core::{fmt, num};

        use super::SHA1_DIGEST_STR_LEN;

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
                    Len(len) => {
                        write!(f, "invalid length (have {len}, want {SHA1_DIGEST_STR_LEN})")
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

        #[test]
        fn fixture() {
            assert_eq!(
                "123456789abcdef0123456789abcdef012345678"
                    .parse::<Oid>()
                    .unwrap(),
                Oid::from_sha1([
                    0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a,
                    0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                ])
            );
        }

        #[test]
        fn zero() {
            assert_eq!(
                "0000000000000000000000000000000000000000"
                    .parse::<Oid>()
                    .unwrap(),
                Oid::sha1_zero()
            );
        }

        #[quickcheck]
        fn git2_roundtrip(oid: Oid) {
            let other = git2::Oid::from(oid);
            let other = other.to_string();
            let other = other.parse::<Oid>().unwrap();
            assert_eq!(oid, other);
        }

        #[quickcheck]
        fn gix_roundrip(oid: Oid) {
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
                ).fmt(f)
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

        #[test]
        fn fixture() {
            assert_eq!(
                Oid::from_sha1([
                    0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a,
                    0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
                ])
                .to_string(),
                "123456789abcdef0123456789abcdef012345678"
            );
        }

        #[test]
        fn zero() {
            assert_eq!(
                Oid::sha1_zero().to_string(),
                "0000000000000000000000000000000000000000"
            );
        }

        #[quickcheck]
        fn git2(oid: Oid) {
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

    use super::Oid;

    impl From<Other> for Oid {
        fn from(other: Other) -> Self {
            match other {
                Other::Sha1(digest) => Self::Sha1(digest),
            }
        }
    }

    impl From<Oid> for Other {
        fn from(oid: Oid) -> Other {
            match oid {
                Oid::Sha1(digest) => Other::Sha1(digest),
            }
        }
    }

    impl core::cmp::PartialEq<Other> for Oid {
        fn eq(&self, other: &Other) -> bool {
            match (self, other) {
                (Oid::Sha1(a), Other::Sha1(b)) => a == b,
            }
        }
    }

    impl AsRef<gix_hash::oid> for Oid {
        fn as_ref(&self) -> &gix_hash::oid {
            match self {
                Oid::Sha1(digest) => gix_hash::oid::from_bytes_unchecked(digest),
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use gix_hash::Kind;

        #[test]
        fn zero() {
            assert!(Oid::sha1_zero() == Other::null(Kind::Sha1));
        }
    }
}

#[cfg(any(feature = "git2", test))]
mod git2 {
    use ::git2::Oid as Other;

    use super::*;

    const EXPECT: &str = "git2::Oid must be exactly 20 bytes long";

    impl From<Other> for Oid {
        fn from(other: Other) -> Self {
            Self::Sha1(other.as_bytes().try_into().expect(EXPECT))
        }
    }

    impl From<Oid> for Other {
        fn from(oid: Oid) -> Self {
            match oid {
                Oid::Sha1(array) => Other::from_bytes(&array).expect(EXPECT),
            }
        }
    }

    impl From<&Oid> for Other {
        fn from(oid: &Oid) -> Self {
            match oid {
                Oid::Sha1(array) => Other::from_bytes(array).expect(EXPECT),
            }
        }
    }

    impl core::cmp::PartialEq<Other> for Oid {
        fn eq(&self, other: &Other) -> bool {
            other.as_bytes() == AsRef::<[u8]>::as_ref(&self)
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn zero() {
            assert!(Oid::sha1_zero() == Other::zero());
        }
    }
}

#[cfg(any(test, feature = "qcheck"))]
mod test {
    mod qcheck {
        use ::qcheck::{Arbitrary, Gen};

        use crate::*;

        impl Arbitrary for Oid {
            fn arbitrary(g: &mut Gen) -> Self {
                let slice = [0u8; SHA1_DIGEST_LEN];
                g.fill(slice);
                Self::Sha1(slice)
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
                        use crate::str::SHA1_DIGEST_STR_LEN;
                        write!(f, "a Git object identifier (SHA-1 digest in hexadecimal notation; {SHA1_DIGEST_STR_LEN} characters; {SHA1_DIGEST_LEN} bytes)")
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

    use ::schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};

    use super::Oid;

    impl JsonSchema for Oid {
        fn schema_name() -> Cow<'static, str> {
            "Oid".into()
        }

        fn schema_id() -> Cow<'static, str> {
            concat!(module_path!(), "::Oid").into()
        }

        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            use crate::{str::SHA1_DIGEST_STR_LEN, SHA1_DIGEST_LEN};
            json_schema!({
                "description": format!("A Git object identifier (SHA-1 digest in hexadecimal notation; {SHA1_DIGEST_STR_LEN} characters; {SHA1_DIGEST_LEN} bytes)"),
                "type": "string",
                "maxLength": SHA1_DIGEST_STR_LEN,
                "minLength": SHA1_DIGEST_STR_LEN,
                "pattern":  format!("^[0-9a-fA-F]{{{SHA1_DIGEST_STR_LEN}}}$"),
            })
        }
    }
}
