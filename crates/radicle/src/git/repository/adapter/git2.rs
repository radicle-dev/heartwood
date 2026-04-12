//! The [`Repository`] adapters for the various repository traits.
//!
//! [`Repository`]: crate::git::raw::Repository

use crate::git;
use crate::git::raw;

use crate::git::repository::ObjectKind;

mod ancestry;
mod object;
mod reference;
mod revwalk;

/// Helper trait to enable method chaining to return `None` when the error
/// matches [`ErrorCode::NotFound`].
///
/// [`ErrorCode::NotFound`]: crate::git::raw::ErrorCode::NotFound
trait NotFound<T> {
    fn or_is_not_found(self) -> Result<Option<T>, raw::Error>;
}

impl<T> NotFound<T> for Result<T, raw::Error> {
    fn or_is_not_found(self) -> Result<Option<T>, git::raw::Error> {
        self.map(|t| Ok(Some(t))).unwrap_or_else(|e| {
            if matches!(e.code(), raw::ErrorCode::NotFound) {
                Ok(None)
            } else {
                Err(e)
            }
        })
    }
}

/// Map a [`raw::ObjectType`] to an [`ObjectKind`].
fn object_kind(kind: raw::ObjectType) -> ObjectKind {
    match kind {
        raw::ObjectType::Blob => ObjectKind::Blob,
        raw::ObjectType::Tree => ObjectKind::Tree,
        raw::ObjectType::Commit => ObjectKind::Commit,
        raw::ObjectType::Tag => ObjectKind::Tag,
        raw::ObjectType::Any => unreachable!("git2 does not expose other object types"),
    }
}
