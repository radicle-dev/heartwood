//! This module contains definitions for use with the `sqlite` crate.

/// Value for a `journal_mode` pragma statement.
/// For a description of all variants please refer to
/// <https://sqlite.org/pragma.html#pragma_journal_mode>.
/// Note that when SQLite documentation talks about "the application",
/// the application linked against this crate, e.g. Radicle Node, Radicle CLI,
/// and others, is meant.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum JournalMode {
    #[default]
    DELETE,
    TRUNCATE,
    PERSIST,
    MEMORY,
    WAL,
    OFF,
}

impl std::fmt::Display for JournalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DELETE => "DELETE",
            Self::TRUNCATE => "TRUNCATE",
            Self::PERSIST => "PERSIST",
            Self::MEMORY => "MEMORY",
            Self::WAL => "WAL",
            Self::OFF => "OFF",
        })
    }
}
