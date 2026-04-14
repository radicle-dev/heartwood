//! Git repository abstraction layer.
//!
//! Provides a library-agnostic interface for git repository operations,
//! separating concerns into:
//!
//! - [`ancestry`] – Git ancestry operations.
//! - [`types`] — Git domain types, i.e. Blob, Commit, TreeEntry, etc.
//! - [`object`] — The Git object store; providing read and write capabilities of Git objects.
//! - [`reference`] — The Git reference store; providing read and write capabilities of Git references.
//! - [`revwalk`] – Git commit graph walk operations, i.e. "revwalk".
//!
//! [`reference`]: self::reference

pub mod ancestry;
pub mod object;
pub mod reference;
pub mod revwalk;
pub mod types;
pub mod user;

mod adapter;

pub use ancestry::{AheadBehind, Ancestry};
pub use revwalk::{Revwalk, RevwalkPlan, SortOrder};
pub use types::{Blob, Commit, ObjectKind, TreeEntry};
