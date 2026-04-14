//! Domain data for creating signed reference updates.
//!
//! [`Committer`] is used for the author of the commit in a signed references commit.
//! It provides a way to create a stable author and timestamp for deterministic commits.

use std::path::Path;

#[cfg(test)]
mod properties;

use crypto::PublicKey;
use radicle_git_metadata::author::Author;
use radicle_git_metadata::author::Time;

use crate::git::repository::types::TreeEntry;

/// A [`TreeEntry`] for the signed references payload blob.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RefsEntry(TreeEntry);

impl RefsEntry {
    /// Create a new entry with the canonical refs bytes.
    pub fn new(content: Vec<u8>) -> Self {
        Self(TreeEntry::Blob {
            path: Path::new(crate::storage::refs::REFS_BLOB_PATH).into(),
            content,
        })
    }

    /// Unwrap into the underlying [`TreeEntry`].
    pub fn into_inner(self) -> TreeEntry {
        self.0
    }
}

/// A [`TreeEntry`] for the cryptographic signature blob.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct SignatureEntry(TreeEntry);

impl SignatureEntry {
    /// Create a new entry with the signature bytes.
    pub fn new(content: Vec<u8>) -> Self {
        Self(TreeEntry::Blob {
            path: Path::new(crate::storage::refs::SIGNATURE_BLOB_PATH).into(),
            content,
        })
    }

    /// Unwrap into the underlying [`TreeEntry`].
    pub fn into_inner(self) -> TreeEntry {
        self.0
    }
}

/// Convenience type that corresponds to an [`Author`].
///
/// Most users will want to instantiate this via [`Committer::from_env_or_now`],
/// which automatically constructs a stable [`Author`] for tests as well.
///
/// Otherwise, an [`Author`] can be provided via [`Committer::new`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Committer {
    pub author: Author,
}

impl Committer {
    const NAME: &str = "radicle";

    /// Construct a [`Committer`] using the timestamp found at
    /// [`GIT_COMMITTER_DATE`],
    ///
    /// If [`GIT_COMMITTER_DATE`] is unset, it uses the current system
    /// time.
    ///
    /// The given [`PublicKey`] is always used for the email.
    ///
    /// In test code, `Committer::stable` is returned.
    ///
    /// [`GIT_COMMITTER_DATE`]: crate::profile::env::GIT_COMMITTER_DATE
    pub fn from_env_or_now(public_key: &PublicKey) -> Self {
        #[cfg(any(test, feature = "test"))]
        return Self::stable(public_key);

        #[cfg(not(any(test, feature = "test")))]
        {
            use crate::profile::env::GIT_COMMITTER_DATE;
            use std::env::VarError;
            use std::env::var;

            let timestamp = match var(GIT_COMMITTER_DATE) {
                Ok(s) => match s.trim().parse::<u64>() {
                    Ok(timestamp) => timestamp,
                    Err(err) => {
                        panic!(
                            "Value of environment variable `{}` does not parse as integer: {err}",
                            GIT_COMMITTER_DATE
                        );
                    }
                },
                Err(VarError::NotPresent) => std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .expect("time is later than unix epoch")
                    .as_secs(),
                Err(VarError::NotUnicode(_)) => {
                    panic!(
                        "Value for environment variable `{}` is not valid Unicode.",
                        GIT_COMMITTER_DATE
                    );
                }
            };

            let timestamp = timestamp
                .try_into()
                .expect("seconds since unix epoch must fit i64");

            Self::from_key_and_time(public_key, timestamp)
        }
    }

    /// Provide a stable [`Committer`] with the same `name`, `email`, and `time`
    /// values.
    ///
    /// The [`Time`] value is constructed using the same seconds value used for
    /// other tests. These values are set via the `RAD_LOCAL_TIME` environment
    /// variable.
    #[cfg(any(test, feature = "test"))]
    pub fn stable(public_key: &PublicKey) -> Self {
        Self::from_key_and_time(public_key, 1671125284)
    }

    /// Construct a [`Committer`] with the provided [`Author`].
    pub fn new(author: Author) -> Self {
        Self { author }
    }

    pub fn into_inner(self) -> Author {
        self.author
    }

    fn from_key_and_time(public_key: &PublicKey, timestamp: i64) -> Self {
        Self::new(Author {
            name: Self::NAME.to_string(),
            email: public_key.to_human(),
            time: Time::new(timestamp, 0),
        })
    }
}
