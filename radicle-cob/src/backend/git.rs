// Copyright © 2022 The Radicle Team

pub mod change;

#[cfg(feature = "stable-commit-ids")]
pub mod stable;

/// Environment variable to set to overwrite the commit date for both the author and the committer.
///
/// The format must be a unix timestamp.
pub const GIT_COMMITTER_DATE: &str = "GIT_COMMITTER_DATE";
