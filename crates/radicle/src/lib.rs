#![allow(clippy::too_many_arguments)]
#![deny(clippy::unwrap_used)]

pub extern crate radicle_crypto as crypto;

extern crate radicle_localtime as localtime;

mod canonical;

pub mod cli;
pub mod cob;
pub mod collections;
pub mod explorer;
pub mod git;
pub mod identity;
pub mod io;
pub mod node;
pub mod profile;
pub mod rad;
#[cfg(feature = "schemars")]
pub mod schemars_ext;
pub mod serde_ext;
pub mod sql;
pub mod storage;
#[cfg(any(test, feature = "test"))]
pub mod test;
pub mod version;
pub mod web;

pub use cob::{external, issue, patch};
pub use node::Node;
pub use profile::Profile;
pub use storage::git::Storage;

pub mod prelude {
    use super::*;

    pub use crypto::PublicKey;
    pub use git::BranchName;
    pub use identity::{Did, Doc, RawDoc, RepoId, project::Project};
    pub use node::{Alias, NodeId, Timestamp};
    pub use profile::Profile;
    pub use storage::{ReadRepository, ReadStorage, SignRepository, WriteRepository, WriteStorage};
}
