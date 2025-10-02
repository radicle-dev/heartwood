#[cfg(feature = "git2")]
pub mod identity;
#[cfg(feature = "git2")]
pub use identity::{Person, Project, RemoteProject};

#[cfg(feature = "git2")]
pub mod storage;
#[cfg(feature = "git2")]
pub use storage::Storage;

pub mod arbitrary;
