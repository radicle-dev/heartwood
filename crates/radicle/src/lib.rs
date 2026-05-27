#![allow(clippy::too_many_arguments)]
#![deny(clippy::unwrap_used)]

pub extern crate radicle_crypto as crypto;

#[macro_use]
extern crate amplify;

extern crate radicle_localtime as localtime;

#[cfg_attr(creusot, creusot_std::prelude::trusted)]
mod canonical;

#[cfg(not(creusot))]
pub mod cli;
pub mod cob;
#[cfg(not(creusot))]
pub mod collections;
#[cfg(not(creusot))]
pub mod explorer;
pub mod git;
pub mod identity;
#[cfg(not(creusot))]
pub mod io;
#[cfg(not(creusot))]
pub mod node;
#[cfg(not(creusot))]
pub mod profile;
#[cfg(not(creusot))]
pub mod rad;
#[cfg(feature = "schemars")]
pub mod schemars_ext;
#[cfg(not(creusot))]
pub mod serde_ext;
#[cfg(not(creusot))]
pub mod sql;
#[cfg(not(creusot))]
pub mod storage;
#[cfg(all(any(test, feature = "test"), not(creusot)))]
pub mod test;
#[cfg(not(creusot))]
pub mod version;
#[cfg(not(creusot))]
pub mod web;

#[cfg(not(creusot))]
pub use cob::{external, issue, patch};
#[cfg(not(creusot))]
pub use node::Node;
#[cfg(not(creusot))]
pub use profile::Profile;
#[cfg(not(creusot))]
pub use storage::git::Storage;

pub mod prelude {
    use super::*;

    pub use crypto::PublicKey;

    pub use identity::Did;

    #[cfg(not(creusot))]
    pub use git::BranchName;

    #[cfg(not(creusot))]
    pub use identity::{Doc, RawDoc, RepoId, project::Project};

    #[cfg(not(creusot))]
    pub use node::{Alias, NodeId, Timestamp};

    #[cfg(not(creusot))]
    pub use profile::Profile;

    #[cfg(not(creusot))]
    pub use storage::{ReadRepository, ReadStorage, SignRepository, WriteRepository, WriteStorage};
}

#[cfg(creusot)]
use creusot_std::prelude::{requires,ensures,invariant,snapshot};

#[cfg(creusot)]
#[requires(n@ < 1000)]
#[ensures(result@ == n@ * (n@ + 1) / 2)]
pub fn sum_first_n(n: u32) -> u32 {
    let mut sum = 0;
    #[invariant(sum@ * 2 == produced.len() * (produced.len() + 1))]
    for i in 1..=n {
        sum += i;
    }
    sum
}

