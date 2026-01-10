use std::{convert::TryFrom, str};

use radicle_git_ext::Oid;
use thiserror::Error;

#[cfg(feature = "serde")]
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Error)]
pub enum Error {
    /// When trying to get the summary for a [`git2::Commit`] some action
    /// failed.
    #[error("an error occurred trying to get a commit's summary")]
    MissingSummary,
    #[error(transparent)]
    Utf8Error(#[from] str::Utf8Error),
}

/// Represents the authorship of actions in a git repo.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Author {
    /// Name of the author.
    pub name: String,
    /// Email of the author.
    pub email: String,
    /// Time the action was taken, e.g. time of commit.
    #[cfg_attr(
        feature = "serde",
        serde(
            serialize_with = "serialize_time",
            deserialize_with = "deserialize_time"
        )
    )]
    pub time: Time,
}

/// Time used in the authorship of an action in a git repo.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    inner: git2::Time,
}

impl From<git2::Time> for Time {
    fn from(inner: git2::Time) -> Self {
        Self { inner }
    }
}

impl Time {
    pub fn new(epoch_seconds: i64, offset_minutes: i32) -> Self {
        git2::Time::new(epoch_seconds, offset_minutes).into()
    }

    /// Returns the seconds since UNIX epoch.
    pub fn seconds(&self) -> i64 {
        self.inner.seconds()
    }

    /// Returns the timezone offset in minutes.
    pub fn offset_minutes(&self) -> i32 {
        self.inner.offset_minutes()
    }
}

#[cfg(feature = "serde")]
fn deserialize_time<'de, D>(deserializer: D) -> Result<Time, D::Error>
where
    D: Deserializer<'de>,
{
    let seconds: i64 = Deserialize::deserialize(deserializer)?;
    Ok(Time::new(seconds, 0))
}

#[cfg(feature = "serde")]
fn serialize_time<S>(t: &Time, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(t.seconds())
}

impl std::fmt::Debug for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::cmp::Ordering;
        let time = match self.time.offset_minutes().cmp(&0) {
            Ordering::Equal => format!("{}", self.time.seconds()),
            Ordering::Greater => format!("{}+{}", self.time.seconds(), self.time.offset_minutes()),
            Ordering::Less => format!("{}{}", self.time.seconds(), self.time.offset_minutes()),
        };
        f.debug_struct("Author")
            .field("name", &self.name)
            .field("email", &self.email)
            .field("time", &time)
            .finish()
    }
}

impl TryFrom<git2::Signature<'_>> for Author {
    type Error = str::Utf8Error;

    fn try_from(signature: git2::Signature) -> Result<Self, Self::Error> {
        let name = str::from_utf8(signature.name_bytes())?.into();
        let email = str::from_utf8(signature.email_bytes())?.into();
        let time = signature.when().into();

        Ok(Author { name, email, time })
    }
}

/// `Commit` is the metadata of a [Git commit][git-commit].
///
/// [git-commit]: https://git-scm.com/book/en/v2/Git-Internals-Git-Objects
#[cfg_attr(feature = "serde", derive(Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Commit {
    /// Object Id
    pub id: Oid,
    /// The author of the commit.
    pub author: Author,
    /// The actor who committed this commit.
    pub committer: Author,
    /// The long form message of the commit.
    pub message: String,
    /// The summary message of the commit.
    pub summary: String,
    /// The parents of this commit.
    pub parents: Vec<Oid>,
}

impl Commit {
    /// Returns the commit description text. This is the text after the one-line
    /// summary.
    #[must_use]
    pub fn description(&self) -> &str {
        self.message
            .strip_prefix(&self.summary)
            .unwrap_or(&self.message)
            .trim()
    }
}

#[cfg(feature = "serde")]
impl Serialize for Commit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Commit", 7)?;
        state.serialize_field("id", &self.id.to_string())?;
        state.serialize_field("author", &self.author)?;
        state.serialize_field("committer", &self.committer)?;
        state.serialize_field("summary", &self.summary)?;
        state.serialize_field("message", &self.message)?;
        state.serialize_field("description", &self.description())?;
        state.serialize_field(
            "parents",
            &self
                .parents
                .iter()
                .map(|oid| oid.to_string())
                .collect::<Vec<String>>(),
        )?;
        state.end()
    }
}

impl TryFrom<git2::Commit<'_>> for Commit {
    type Error = Error;

    fn try_from(commit: git2::Commit) -> Result<Self, Self::Error> {
        let id = commit.id().into();
        let author = Author::try_from(commit.author())?;
        let committer = Author::try_from(commit.committer())?;
        let message_raw = commit.message_bytes();
        let message = str::from_utf8(message_raw)?.into();
        let summary_raw = commit.summary_bytes().ok_or(Error::MissingSummary)?;
        let summary = str::from_utf8(summary_raw)?.into();
        let parents = commit.parent_ids().map(|oid| oid.into()).collect();

        Ok(Commit {
            id,
            author,
            committer,
            message,
            summary,
            parents,
        })
    }
}
