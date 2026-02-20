use super::sqlite_ext::*;

const DEFAULT_JOURNAL_MODE: JournalMode = JournalMode::WAL;
const DEFAULT_SYNCHRONOUS: Synchronous = Synchronous::FULL;

/// Database configuration.
#[derive(Debug, Default, Copy, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Config {
    /// SQLite configuration.
    #[serde(skip_serializing_if = "crate::serde_ext::is_default")]
    pub sqlite: SqliteConfig,
}

/// SQLite database configuration.
#[derive(Debug, Default, Copy, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SqliteConfig {
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
    pub pragma: Pragma,
}

/// Global SQLite pragma statements to make in order to configure SQLite itself,
/// see <https://sqlite.org/pragma.html>.
#[derive(Debug, Copy, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Pragma {
    #[serde(
        default = "serde::journal_mode::default",
        skip_serializing_if = "serde::journal_mode::is_default"
    )]
    pub journal_mode: JournalMode,

    #[serde(
        default = "serde::synchronous::default",
        skip_serializing_if = "serde::synchronous::is_default"
    )]
    pub synchronous: Synchronous,
}

pub mod serde {
    use super::*;

    pub mod journal_mode {
        use super::*;

        pub fn default() -> JournalMode {
            DEFAULT_JOURNAL_MODE
        }

        pub fn is_default(journal_mode: &JournalMode) -> bool {
            matches!(journal_mode, &DEFAULT_JOURNAL_MODE)
        }
    }

    pub mod synchronous {
        use super::*;

        pub fn default() -> Synchronous {
            DEFAULT_SYNCHRONOUS
        }

        pub fn is_default(synchronous: &Synchronous) -> bool {
            matches!(synchronous, &DEFAULT_SYNCHRONOUS)
        }
    }
}

impl Default for Pragma {
    fn default() -> Self {
        Self {
            journal_mode: DEFAULT_JOURNAL_MODE,
            synchronous: DEFAULT_SYNCHRONOUS,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::assert_matches;

    use super::*;
    use serde_json::json;

    #[test]
    fn database_config_valid_combinations() {
        let cases = [
            (None, None, JournalMode::WAL, Synchronous::FULL),
            (
                Some("WAL"),
                Some("NORMAL"),
                JournalMode::WAL,
                Synchronous::NORMAL,
            ),
            (
                Some("WAL"),
                Some("FULL"),
                JournalMode::WAL,
                Synchronous::FULL,
            ),
            (Some("WAL"), Some("OFF"), JournalMode::WAL, Synchronous::OFF),
            (
                Some("DELETE"),
                Some("FULL"),
                JournalMode::DELETE,
                Synchronous::FULL,
            ),
            (
                Some("DELETE"),
                Some("EXTRA"),
                JournalMode::DELETE,
                Synchronous::EXTRA,
            ),
            (
                Some("WAL"),
                Some("NORMAL"),
                JournalMode::WAL,
                Synchronous::NORMAL,
            ),
            (
                Some("DELETE"),
                Some("NORMAL"),
                JournalMode::DELETE,
                Synchronous::NORMAL,
            ),
        ];

        for (journal_mode, synchronous, expected_journal_mode, expected_synchronous) in cases {
            let mut config = json!({});

            if let Some(journal_mode) = journal_mode {
                config["pragma"]["journalMode"] = json!(journal_mode);
            }

            if let Some(synchronous) = synchronous {
                config["pragma"]["synchronous"] = json!(synchronous);
            }

            #[allow(clippy::unwrap_used)]
            let config: SqliteConfig = serde_json::from_value(config).unwrap();

            assert_eq!(
                config.pragma,
                Pragma {
                    journal_mode: expected_journal_mode,
                    synchronous: expected_synchronous
                }
            );
        }
    }

    #[test]
    fn invalid() {
        let invalid_cases = [
            (Some("INVALID"), Some("NORMAL"), "invalid journal_mode"),
            (Some("WAL"), Some("INVALID"), "invalid synchronous"),
            (Some("WAL"), Some("normal"), "lowercase synchronous"),
            (Some("Wal"), Some("NORMAL"), "mixed case journal_mode"),
        ];

        for (journal_mode, synchronous, description) in invalid_cases {
            let mut pragma = json!({});

            if let Some(journal_mode) = journal_mode {
                pragma["journalMode"] = json!(journal_mode);
            }

            if let Some(synchronous) = synchronous {
                pragma["synchronous"] = json!(synchronous);
            }

            assert_matches!(
                serde_json::from_value::<Pragma>(pragma),
                Err(_),
                "{}",
                description,
            );
        }
    }
}
