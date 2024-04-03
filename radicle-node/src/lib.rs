pub mod bounded;
pub mod control;
pub mod deserializer;
pub mod runtime;
pub mod service;
#[cfg(any(test, feature = "test"))]
pub mod test;
#[cfg(test)]
pub mod tests;
pub mod wire;
pub mod worker;

use radicle::version::Version;

pub use localtime::{LocalDuration, LocalTime};
pub use netservices::Direction as Link;
pub use radicle::node::PROTOCOL_VERSION;
pub use radicle::prelude::Timestamp;
pub use radicle::{collections, crypto, git, identity, node, profile, rad, storage};
pub use runtime::Runtime;

/// Node version.
pub const VERSION: Version = Version {
    name: env!("CARGO_PKG_NAME"),
    commit: env!("GIT_HEAD"),
    version: env!("RADICLE_VERSION"),
    timestamp: env!("GIT_COMMIT_TIME"),
};

pub mod prelude {
    pub use crate::bounded::BoundedVec;
    pub use crate::crypto::{PublicKey, Signature, Signer};
    pub use crate::deserializer::Deserializer;
    pub use crate::identity::{Did, RepoId};
    pub use crate::node::Address;
    pub use crate::service::filter::Filter;
    pub use crate::service::{DisconnectReason, Event, Message, Network, NodeId};
    pub use crate::storage::refs::Refs;
    pub use crate::storage::WriteStorage;
    pub use crate::{LocalDuration, LocalTime, Timestamp};
}
