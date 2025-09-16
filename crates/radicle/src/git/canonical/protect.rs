//! Some reference names are protected and cannot be used with canonical
//! references. This module contains checks for these cases.
//!
//! Protected references are:
//!  1. `refs/rad`
//!  2. Any reference matching `refs/rad/*`, e.g. `refs/rad/id`, `refs/rad/foo/bar`.

const REFS_RAD: &str = "refs/rad";

/// Reference-like types, which we encounter when working with canonical references.
pub(crate) trait RefLike: AsRef<str> + Ord + std::fmt::Display + serde::Serialize {}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reference-like string '{REFS_RAD}' is protected")]
    RefsRad,
    #[error("reference-like string '{reflike}' is protected because it starts with '{REFS_RAD}/'")]
    RefsRadChild { reflike: String },
}

/// A witnesses that the inner reference-like value is not protected.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub(super) struct Unprotected<T: RefLike>(T);

impl<T: RefLike> Unprotected<T> {
    pub fn new(reflike: T) -> Result<Self, Error> {
        match reflike
            .as_ref()
            .strip_prefix(REFS_RAD)
            .map(|rest| rest.get(..1))
        {
            Some(None) => Err(Error::RefsRad),
            Some(Some("/")) => Err(Error::RefsRadChild {
                reflike: reflike.to_string(),
            }),
            Some(_) | None => Ok(Self(reflike)),
        }
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: RefLike> AsRef<T> for Unprotected<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<'de, T: RefLike + serde::Deserialize<'de>> serde::Deserialize<'de> for Unprotected<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(T::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<T: RefLike + std::fmt::Display> std::fmt::Display for Unprotected<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// For types that are commonly used in conjunction with [`Unprotected`]
/// have some `impl`s and companion infallible injections.
mod impls {
    use crate::git::{refspec::QualifiedPattern, RefString};

    use super::*;

    /// [`RefString`] models reference names, thus the prototype of what it
    /// means to be [`RefLike`].
    impl RefLike for RefString {}

    /// A [`QualifiedPattern`] is [`RefLike`] in the sense that it matches a
    /// (possibly infinite) set of [`crate::git::Qualified`].
    impl RefLike for QualifiedPattern<'_> {}
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::assert_matches;
    use crate::git::refname;

    use super::{Error::*, Unprotected};

    #[test]
    fn refs_rad() {
        assert_matches!(Unprotected::new(refname!("refs/rad")), Err(RefsRad))
    }

    #[test]
    fn refs_rad_id() {
        assert_matches!(
            Unprotected::new(refname!("refs/rad/id")),
            Err(RefsRadChild { .. })
        )
    }

    #[test]
    fn refs_radieschen() {
        assert_matches!(Unprotected::new(refname!("refs/radieschen")), Ok(_))
    }
}
