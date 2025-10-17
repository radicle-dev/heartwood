// Copyright © 2022 The Radicle Link Contributors

#[cfg(feature = "git2")]
pub mod git;

#[cfg(feature = "stable-commit-ids")]
pub mod stable;

/// Environment variable to set to overwrite the commit date for both the author and the committer.
///
/// The format must be a unix timestamp.
pub const GIT_COMMITTER_DATE: &str = "GIT_COMMITTER_DATE";