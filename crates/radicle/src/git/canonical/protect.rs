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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, Hash)]
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

    /// Allows creation without any checking. Callers must ensure that
    /// `reflike` is indeed unprotected!
    #[inline]
    const fn new_unchecked(reflike: T) -> Self {
        Self(reflike)
    }
}

impl<T: RefLike> AsRef<T> for Unprotected<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: RefLike> std::borrow::Borrow<T> for Unprotected<T> {
    fn borrow(&self) -> &T {
        &self.0
    }
}

/// Enables looking up entries in a map keyed by `Unprotected<RefString>` using
/// a `&RefStr`.
impl std::borrow::Borrow<crate::git::fmt::RefStr> for Unprotected<crate::git::fmt::RefString> {
    fn borrow(&self) -> &crate::git::fmt::RefStr {
        self.0.as_ref()
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
    use crate::git::{
        fmt::{Qualified, RefStr, RefString, refname, refspec::QualifiedPattern},
        refs::branch,
    };

    use super::*;

    /// [`RefString`] models reference names, thus the prototype of what it
    /// means to be [`RefLike`].
    impl RefLike for RefString {}

    impl Unprotected<RefString> {
        /// The reference name `HEAD`.
        // We would like to have a `pub const HEAD`, but
        // [`crate::git::RefStr::from_str`] is private.
        #[inline]
        pub fn head() -> Self {
            // Calling [`Unprotected::new_unchecked`] here is legal,
            // because we know statically that `HEAD` is not protected.
            Unprotected::new_unchecked(refname!("HEAD"))
        }
    }

    /// [`Qualified`] is a restriction on [`RefString`].
    impl RefLike for Qualified<'_> {}

    impl Unprotected<Qualified<'_>> {
        /// Construct a qualified reference name for given branch, i.e.,
        /// return `/refs/heads/<name>`
        pub fn branch(name: &RefStr) -> Self {
            Self::new(branch(name)).expect("branches are never protected")
        }

        pub fn to_ref_string(&self) -> Unprotected<RefString> {
            Unprotected::new_unchecked(self.0.to_ref_string())
        }
    }

    /// A [`QualifiedPattern`] is [`RefLike`] in the sense that it matches a
    /// (possibly infinite) set of [`crate::git::Qualified`].
    impl RefLike for QualifiedPattern<'_> {}
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::assert_matches;
    use crate::git::fmt::refname;

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
