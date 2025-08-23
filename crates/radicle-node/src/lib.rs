// N.b. Rust 1.85 introduced some annoying clippy warnings about using `b""`
// syntax in place of `b''`, but in our cases they were u8 and not [u8] so the
// suggestions did not make sense.
#![allow(clippy::byte_char_slices)]

pub mod fingerprint;
pub mod reactor;
pub mod runtime;

mod control;
pub(crate) use radicle_protocol::service;
mod wire;
mod worker;

#[cfg(any(test, feature = "test"))]
pub mod test;
#[cfg(test)]
pub mod tests;

use std::str::FromStr;
use std::sync::LazyLock;

use radicle::version::Version;

pub use localtime::{LocalDuration, LocalTime};
pub use radicle::node::Link;
pub use radicle::node::UserAgent;
pub use radicle::node::PROTOCOL_VERSION;
pub use radicle::prelude::Timestamp;
pub use radicle::{collections, crypto, git, identity, node, profile, rad, storage};
pub use runtime::Runtime;

/// Node version.
pub const VERSION: Version = Version {
    name: env!("CARGO_PKG_NAME"),
    commit: env!("GIT_HEAD"),
    version: env!("RADICLE_VERSION"),
    timestamp: env!("SOURCE_DATE_EPOCH"),
};

/// This node's user agent string.
pub static USER_AGENT: LazyLock<UserAgent> = LazyLock::new(|| {
    FromStr::from_str(format!("/radicle:{}/", VERSION.version).as_str())
        .expect("user agent is valid")
});

pub mod prelude {
    pub use crate::crypto::{PublicKey, Signature};
    pub use crate::identity::{Did, RepoId};
    pub use crate::node::{config::Network, Address, Event, NodeId};
    pub use crate::service::filter::Filter;
    pub use crate::service::{DisconnectReason, Message};
    pub use crate::storage::refs::Refs;
    pub use crate::storage::WriteStorage;
    pub use crate::{LocalDuration, LocalTime, Timestamp};
}
