//! Git repository abstraction layer.
//!
//! Provides a library-agnostic interface for git repository operations,
//! separating concerns into:
//!
//! - [`types`] — Git domain types, i.e. Blob, Commit, TreeEntry, etc.
//! - [`object`] — The Git object store; providing read and write capabilities of Git objects.
//! - [`reference`] — The Git reference store; providing read and write capabilities of Git references.
//!
//! [`reference`]: self::reference

pub mod object;
pub mod reference;
pub mod types;

pub use types::{Blob, Commit, ObjectKind, TreeEntry};
