use std::num::NonZeroU64;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use std::fmt::Display;
use thiserror::Error;

/// A [`RangeInclusive`] of all valid identity document versions, known to this
/// release of the protocol.
pub const KNOWN_VERSIONS: RangeInclusive<Version> = Version::MIN..=Version::LATEST;

/// The version number of the repository identity documents.
///
/// The number cannot be zero and will range from `1` to the maximum known
/// version number for this given release. This range can be retrieved from the
/// [`KNOWN`] range.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Version(NonZeroU64);

impl Version {
    /// The minimum `Version` is version `1`.
    pub const MIN: Version = Version(unsafe { NonZeroU64::new_unchecked(1) });

    /// The current latest `Version` is version `2`.
    pub const LATEST: Version = Version(unsafe { NonZeroU64::new_unchecked(2) });

    /// Get the latest known `Version`.
    pub const fn latest() -> Version {
        Self::LATEST
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Version> for u64 {
    fn from(version: Version) -> Self {
        version.0.into()
    }
}

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("encountered unexpected identity document version {actual}, expected {expected}")]
    Unexpected { expected: Version, actual: Version },
}

impl VersionError {
    /// Provide a verbose error.
    ///
    /// This will give a user more information on how to upgrade to a newer
    /// version of an identity document, if there is one.
    pub fn verbose(&self) -> String {
        const UNKOWN_VERSION_ERROR: &str = r#"
Perhaps a new version of the identity document is released which is not supported by the current client.
See https://radicle.xyz for the latest versions of Radicle.
The CLI command `rad id migrate` will help to migrate to an up-to-date versions."#;

        format!("{self}{UNKOWN_VERSION_ERROR}")
    }
}

macro_rules! version {
    ($def: expr, $name: ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(try_from = "Version", into = "Version")]
        pub struct $name;

        impl TryFrom<Version> for $name {
            type Error = VersionError;

            fn try_from(value: Version) -> Result<Self, Self::Error> {
                if value == $def {
                    Ok(Self)
                } else {
                    Err(VersionError::Unexpected {
                        expected: $def,
                        actual: value,
                    })
                }
            }
        }

        impl From<$name> for Version {
            fn from(_: $name) -> Self {
                $def
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                $def.fmt(formatter)
            }
        }
    };
}

version!(*KNOWN_VERSIONS.start(), VersionOne);
version!(*KNOWN_VERSIONS.end(), VersionTwo);

impl Default for VersionOne {
    fn default() -> Self {
        debug_assert_eq!(Into::<Version>::into(Self), *KNOWN_VERSIONS.start());
        Self
    }
}
