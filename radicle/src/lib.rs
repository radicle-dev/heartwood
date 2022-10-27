#![allow(clippy::match_like_matches_macro)]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub extern crate radicle_crypto as crypto;

pub mod collections;
pub mod git;
pub mod hash;
pub mod identity;
pub mod keystore;
pub mod node;
pub mod profile;
pub mod rad;
pub mod serde_ext;
#[cfg(feature = "sql")]
pub mod sql;
pub mod storage;
#[cfg(any(test, feature = "test"))]
pub mod test;

pub use keystore::UnsafeKeystore;
pub use profile::Profile;
pub use storage::git::Storage;
