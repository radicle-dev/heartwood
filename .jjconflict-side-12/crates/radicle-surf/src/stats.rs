#[cfg(feature = "serde")]
use serde::Serialize;

/// Stats for a repository
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Number of commits
    pub commits: usize,
    /// Number of local branches
    pub branches: usize,
    /// Number of contributors
    pub contributors: usize,
}
