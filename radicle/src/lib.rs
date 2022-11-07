#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::explicit_auto_deref)] // TODO: This can be removed when the clippy bugs are fixed
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub extern crate radicle_crypto as crypto;

pub mod cob;
pub mod collections;
pub mod git;
pub mod hash;
pub mod identity;
pub mod node;
pub mod profile;
pub mod rad;
pub mod serde_ext;
#[cfg(feature = "sql")]
pub mod sql;
pub mod storage;
#[cfg(any(test, feature = "test"))]
pub mod test;

pub use profile::Profile;
pub use storage::git::Storage;

pub mod prelude {
    use super::*;

    pub use crypto::{Signer, Verified};
    pub use identity::{Doc, Id};
    pub use node::NodeId;
    pub use profile::Profile;
    pub use storage::BranchName;
}
