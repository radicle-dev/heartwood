use alloc::fmt;
use alloc::string::String;
use alloc::string::ToString as _;
use alloc::vec::Vec;

use radicle_oid::Oid;
use thiserror::Error;

/// Radicle identifier prefix.
pub const RAD_PREFIX: &str = "rad:";

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum IdError {
    #[error(transparent)]
    Multibase(#[from] multibase::Error),
    #[error("invalid length: expected {expected} bytes, got {actual} bytes")]
    Length { expected: usize, actual: usize },
    #[error(fmt = fmt_mismatched_base_encoding)]
    MismatchedBaseEncoding {
        input: String,
        expected: Vec<multibase::Base>,
        found: multibase::Base,
    },
}

fn fmt_mismatched_base_encoding(
    input: &String,
    expected: &[multibase::Base],
    found: &multibase::Base,
    formatter: &mut fmt::Formatter,
) -> fmt::Result {
    write!(
        formatter,
        "invalid multibase encoding '{}' for '{}', expected one of {:?}",
        found.code(),
        input,
        expected.iter().map(|base| base.code()).collect::<Vec<_>>()
    )
}

/// A repository identifier.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RepoId(
    #[cfg_attr(feature = "schemars", schemars(
        with = "String",
        description = "A repository identifier. Starts with \"rad:\", followed by a multibase Base58 encoded Git object identifier.",
        regex(pattern = r"rad:z[1-9a-km-zA-HJ-NP-Z]+"),
        length(min = 5),
        example = &"rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5",
    ))]
    Oid,
);

impl core::fmt::Display for RepoId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.urn().as_str())
    }
}

impl core::fmt::Debug for RepoId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RepoId({self})")
    }
}

impl RepoId {
    const ALLOWED_BASES: [multibase::Base; 1] = [multibase::Base::Base58Btc];

    /// Format the identifier as a human-readable URN.
    ///
    /// Eg. `rad:z3XncAdkZjeK9mQS5Sdc4qhw98BUX`.
    ///
    #[must_use]
    pub fn urn(&self) -> String {
        RAD_PREFIX.to_string() + &self.canonical()
    }

    /// Parse an identifier from the human-readable URN format.
    /// Accepts strings without the prefix [`RAD_PREFIX`] as well,
    /// for convenience.
    pub fn from_urn(s: &str) -> Result<Self, IdError> {
        let s = s.strip_prefix(RAD_PREFIX).unwrap_or(s);
        let id = Self::from_canonical(s)?;

        Ok(id)
    }

    /// Format the identifier as a multibase string.
    ///
    /// Eg. `z3XncAdkZjeK9mQS5Sdc4qhw98BUX`.
    ///
    #[must_use]
    pub fn canonical(&self) -> String {
        multibase::encode(multibase::Base::Base58Btc, AsRef::<[u8]>::as_ref(&self.0))
    }

    /// Decode the input string into a [`RepoId`].
    ///
    /// # Errors
    ///
    /// - The [multibase] decoding fails
    /// - The decoded [multibase] code does not match any expected multibase code
    /// - The input exceeds the expected number of bytes, post multibase decoding
    ///
    /// [multibase]: https://github.com/multiformats/multibase?tab=readme-ov-file#multibase-table
    pub fn from_canonical(input: &str) -> Result<Self, IdError> {
        const EXPECTED_LEN: usize = 20;
        let (base, bytes) = multibase::decode(input)?;
        Self::guard_base_encoding(input, base)?;
        let bytes: [u8; EXPECTED_LEN] =
            bytes.try_into().map_err(|bytes: Vec<u8>| IdError::Length {
                expected: EXPECTED_LEN,
                actual: bytes.len(),
            })?;
        Ok(Self(Oid::from_sha1(bytes)))
    }

    fn guard_base_encoding(input: &str, base: multibase::Base) -> Result<(), IdError> {
        if !Self::ALLOWED_BASES.contains(&base) {
            Err(IdError::MismatchedBaseEncoding {
                input: input.to_string(),
                expected: Self::ALLOWED_BASES.to_vec(),
                found: base,
            })
        } else {
            Ok(())
        }
    }
}

impl core::str::FromStr for RepoId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_urn(s)
    }
}

#[cfg(feature = "std")]
mod std_impls {
    extern crate std;

    use super::{IdError, RepoId};

    use std::ffi::OsString;

    impl TryFrom<OsString> for RepoId {
        type Error = IdError;

        fn try_from(value: OsString) -> Result<Self, Self::Error> {
            let string = value.to_string_lossy();
            Self::from_canonical(&string)
        }
    }

    impl std::hash::Hash for RepoId {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state)
        }
    }
}

impl From<Oid> for RepoId {
    fn from(oid: Oid) -> Self {
        Self(oid)
    }
}

impl core::ops::Deref for RepoId {
    type Target = Oid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "git2")]
mod git2_impls {
    use super::RepoId;

    impl From<git2::Oid> for RepoId {
        fn from(oid: git2::Oid) -> Self {
            Self(oid.into())
        }
    }
}

#[cfg(feature = "gix")]
mod gix_impls {
    use super::RepoId;

    impl From<gix_hash::ObjectId> for RepoId {
        fn from(oid: gix_hash::ObjectId) -> Self {
            Self(oid.into())
        }
    }
}

#[cfg(feature = "radicle-git-ref-format")]
mod radicle_git_ref_format_impls {
    use alloc::string::ToString;

    use radicle_git_ref_format::{Component, RefString};

    use super::RepoId;

    impl From<&RepoId> for Component<'_> {
        fn from(id: &RepoId) -> Self {
            let refstr = RefString::try_from(id.0.to_string())
                .expect("repository id's are valid ref strings");
            Component::from_refstr(refstr).expect("repository id's are valid refname components")
        }
    }
}

#[cfg(feature = "serde")]
mod serde_impls {
    use alloc::string::String;

    use serde::{Deserialize, Deserializer, Serialize, de};

    use super::RepoId;

    impl Serialize for RepoId {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.collect_str(&self.urn())
        }
    }

    impl<'de> Deserialize<'de> for RepoId {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            String::deserialize(deserializer)?
                .parse()
                .map_err(de::Error::custom)
        }
    }

    #[cfg(test)]
    mod test {
        use proptest::proptest;

        use super::super::*;

        fn prop_roundtrip_serde_json(rid: RepoId) {
            let encoded = serde_json::to_string(&rid).unwrap();
            let decoded = serde_json::from_str(&encoded).unwrap();

            assert_eq!(rid, decoded);
        }

        proptest! {
            #[test]
            fn assert_prop_roundtrip_serde_json(rid in arbitrary::rid()) {
                prop_roundtrip_serde_json(rid)
            }
        }
    }
}

#[cfg(feature = "sqlite")]
mod sqlite_impls {
    use alloc::format;
    use alloc::string::ToString;

    use super::RepoId;

    use sqlite::{BindableWithIndex, Error, ParameterIndex, Statement, Value};

    impl TryFrom<&Value> for RepoId {
        type Error = Error;

        fn try_from(value: &Value) -> Result<Self, Self::Error> {
            match value {
                Value::String(id) => RepoId::from_urn(id).map_err(|e| Error {
                    code: None,
                    message: Some(e.to_string()),
                }),
                Value::Binary(_) | Value::Float(_) | Value::Integer(_) | Value::Null => {
                    Err(Error {
                        code: None,
                        message: Some(format!("sql: invalid type `{:?}` for id", value.kind())),
                    })
                }
            }
        }
    }

    impl BindableWithIndex for &RepoId {
        fn bind<I: ParameterIndex>(self, stmt: &mut Statement<'_>, i: I) -> sqlite::Result<()> {
            self.urn().as_str().bind(stmt, i)
        }
    }
}

#[cfg(any(test, feature = "proptest"))]
pub mod arbitrary {
    use proptest::prelude::Strategy;

    use super::RepoId;

    pub fn rid() -> impl Strategy<Value = RepoId> {
        proptest::array::uniform20(proptest::num::u8::ANY)
            .prop_map(|bytes| RepoId::from(radicle_oid::Oid::from_sha1(bytes)))
    }
}

#[cfg(feature = "qcheck")]
impl qcheck::Arbitrary for RepoId {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let bytes = <[u8; 20]>::arbitrary(g);
        let oid = radicle_oid::Oid::from_sha1(bytes);

        RepoId::from(oid)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;
    use proptest::proptest;

    fn prop_roundtrip_parse(rid: RepoId) {
        use core::str::FromStr as _;
        let encoded = rid.to_string();
        let decoded = RepoId::from_str(&encoded).unwrap();

        assert_eq!(rid, decoded);
    }

    proptest! {
        #[test]
        fn assert_prop_roundtrip_parse(rid in arbitrary::rid()) {
            prop_roundtrip_parse(rid)
        }
    }

    #[test]
    fn invalid() {
        assert!("".parse::<RepoId>().is_err());
        assert!("not-a-valid-rid".parse::<RepoId>().is_err());
        assert!(
            "xyz:z3gqcJUoA1n9HaHKufZs5FCSGazv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "RAD:z3gqcJUoA1n9HaHKufZs5FCSGazv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!("rad:".parse::<RepoId>().is_err());
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSG0zv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSGOzv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSGIzv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSGlzv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSGázv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSG@zv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad:Z3gqcJUoA1n9HaHKufZs5FCSGazv5"
                .parse::<RepoId>()
                .is_err()
        );
        assert!("rad:z3gqcJUoA1n9HaHKuf".parse::<RepoId>().is_err());
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5abcdef"
                .parse::<RepoId>()
                .is_err()
        );
        assert!(
            "rad: z3gqcJUoA1n9HaHKufZs5FCSGazv5"
                .parse::<RepoId>()
                .is_err()
        );
    }

    #[test]
    fn valid() {
        assert!(
            "rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5"
                .parse::<RepoId>()
                .is_ok()
        );
        assert!("z3gqcJUoA1n9HaHKufZs5FCSGazv5".parse::<RepoId>().is_ok());
        assert!("z3XncAdkZjeK9mQS5Sdc4qhw98BUX".parse::<RepoId>().is_ok());
    }
}
