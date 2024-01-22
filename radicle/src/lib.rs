#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::iter_nth_zero)]

pub extern crate radicle_crypto as crypto;

#[macro_use]
extern crate amplify;
extern crate radicle_git_ext as git_ext;

mod canonical;

pub mod cli;
pub mod cob;
pub mod collections;
pub mod explorer;
pub mod git;
pub mod identity;
pub mod io;
#[cfg(feature = "logger")]
pub mod logger;
pub mod node;
pub mod profile;
pub mod rad;
pub mod serde_ext;
pub mod sql;
pub mod storage;
#[cfg(any(test, feature = "test"))]
pub mod test;
pub mod version;
pub mod web;

pub use cob::{issue, patch};
pub use node::Node;
pub use profile::Profile;
pub use storage::git::Storage;

pub mod prelude {
    use super::*;

    pub use crypto::{PublicKey, Signer, Verified};
    pub use identity::{project::Project, Did, Doc, RepoId};
    pub use node::{Alias, NodeId, Timestamp};
    pub use profile::Profile;
    pub use storage::{
        BranchName, ReadRepository, ReadStorage, SignRepository, WriteRepository, WriteStorage,
    };
}

pub mod env {
    pub use crypto::env::*;
}
