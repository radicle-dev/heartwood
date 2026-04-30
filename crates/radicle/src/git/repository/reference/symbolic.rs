//! Git symbolic reference operations.
//!
//! The module provides the following traits:
//! - [`Writer`] for writing symbolic references.

use radicle_git_ref_format::{RefStr, RefString};

use super::error;

/// The mode of operation for writing a symbolic reference.
///
/// See [`Writer::write_symbolic_ref`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Set the reference to the given `target`, only if the reference does not
    /// already exist.
    Create { target: RefString },
    /// Set the reference to the given `target`, the reference may exist
    /// already.
    Upsert { target: RefString },
    /// Set the reference to the given `target`, only if the current value of
    /// the reference matches `expected`.
    Cas {
        target: RefString,
        expected: RefString,
    },
}

impl Target {
    /// Construct the [`Create`] variant, which creates a new symbolic reference
    /// pointing to the `target`. This variant will only succeed if the
    /// reference pointing to `target` does not already exist.
    ///
    /// [`Create`]: Target::Create
    pub fn create<R>(target: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self::Create {
            target: target.as_ref().to_ref_string(),
        }
    }

    /// Construct the [`Upsert`] variant, which creates a new symbolic reference
    /// pointing to the `target`. This variant will succeed even if the
    /// reference pointing to `target` already exists.
    ///
    /// [`Upsert`]: Target::Upsert
    pub fn upsert<R>(target: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self::Upsert {
            target: target.as_ref().to_ref_string(),
        }
    }

    /// Construct the [`Cas`] variant, which creates a new symbolic reference
    /// pointing to the `target`. This variant will succeed only when the
    /// `expected` value matches the previously existing target value.
    ///
    /// [`Cas`]: Target::Cas
    pub fn cas<T, E>(target: T, expected: E) -> Self
    where
        T: AsRef<RefStr>,
        E: AsRef<RefStr>,
    {
        Self::Cas {
            target: target.as_ref().to_ref_string(),
            expected: expected.as_ref().to_ref_string(),
        }
    }

    /// The target [`RefString`] that the symbolic reference should point to
    /// after the write.
    pub fn target(&self) -> &RefString {
        match self {
            Self::Create { target } | Self::Upsert { target } | Self::Cas { target, .. } => target,
        }
    }
}

/// Extension trait for symbolic reference support.
///
/// A symbolic reference is one that points to another reference name rather
/// than directly to an [`Oid`] (e.g. `HEAD → refs/heads/main`).
///
/// [`Oid`]: radicle_oid::Oid
pub trait Writer: super::Writer {
    /// Create or update a symbolic reference, identified by `name`, with the
    /// given [`Target`].
    ///
    /// # Errors
    ///
    /// - [`MissingTarget`]: The target reference does not exist in the
    ///   reference database.
    /// - [`ReferenceExists`]: The symbolic reference `name` already exists
    ///   (for [`Target::Create`]).
    /// - [`CasFailed`]: The current target did not match the expected value
    ///   (for [`Target::Cas`]).
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`MissingTarget`]: error::write::WriteSymbolicRef::MissingTarget
    /// [`ReferenceExists`]: error::write::WriteSymbolicRef::ReferenceExists
    /// [`CasFailed`]: error::write::WriteSymbolicRef::CasFailed
    /// [`Backend`]: error::write::WriteSymbolicRef::Backend
    fn write_symbolic_ref<R>(
        &self,
        name: &R,
        target: Target,
        reflog: &str,
    ) -> Result<(), error::write::WriteSymbolicRef>
    where
        R: AsRef<RefStr>;
}

#[cfg(test)]
mod test {
    use radicle_git_ref_format::refname;

    use super::*;

    #[test]
    fn target_create() {
        let t = Target::create(refname!("refs/heads/main"));
        assert_eq!(t.target().as_str(), "refs/heads/main");
        assert!(matches!(t, Target::Create { .. }));
    }

    #[test]
    fn target_upsert() {
        let t = Target::upsert(refname!("refs/heads/main"));
        assert_eq!(t.target().as_str(), "refs/heads/main");
        assert!(matches!(t, Target::Upsert { .. }));
    }

    #[test]
    fn target_cas() {
        let t = Target::cas(refname!("refs/heads/main"), refname!("refs/heads/old"));
        assert_eq!(t.target().as_str(), "refs/heads/main");
        assert!(matches!(t, Target::Cas { expected, .. } if expected.as_str() == "refs/heads/old"));
    }
}
