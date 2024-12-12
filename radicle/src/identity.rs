#![warn(clippy::unwrap_used)]
pub mod did;
pub mod doc;
pub mod project;

pub use crypto::PublicKey;
pub use did::Did;
pub use doc::{Doc, DocAt, DocError, IdError, PayloadError, RawDoc, RepoId, Visibility};
pub use project::Project;

pub use crate::cob::identity::{Action, Error, Identity, IdentityMut, TYPENAME};
