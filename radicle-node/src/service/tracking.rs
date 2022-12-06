mod store;

use std::str::FromStr;

pub use store::{Config, Error};

/// Tracking policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Policy {
    /// The resource is tracked.
    Track,
    /// The resource is blocked.
    Block,
}

/// Tracking scope of a repository tracking policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Track remotes of nodes that are already tracked.
    Trusted,
    /// Track remotes of repository delegates.
    DelegatesOnly,
    /// Track all remotes.
    All,
}

impl FromStr for Scope {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trusted" => Ok(Self::Trusted),
            "delegates-only" => Ok(Self::DelegatesOnly),
            "all" => Ok(Self::All),
            _ => Err(()),
        }
    }
}
