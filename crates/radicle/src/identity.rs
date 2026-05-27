#![deny(clippy::unwrap_used)]
#[cfg(not(creusot))]
pub mod crefs;
pub mod did;
pub mod doc;
#[cfg(not(creusot))]
pub mod project;

#[cfg(not(creusot))]
pub use crefs::CanonicalRefs;
#[cfg(not(creusot))]
pub use crypto::PublicKey;
pub use did::Did;
#[cfg(not(creusot))]
pub use doc::{Doc, DocAt, DocError, IdError, PayloadError, RawDoc, RepoId, Visibility};
#[cfg(not(creusot))]
pub use project::Project;

#[cfg(not(creusot))]
pub use crate::cob::identity::{Action, Error, Identity, IdentityMut, TYPENAME};
