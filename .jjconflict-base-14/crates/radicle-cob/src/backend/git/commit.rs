mod trailers;

use std::fmt;
use std::str::{self, FromStr};

use git2::{ObjectType, Oid};

use metadata::author::Author;
use metadata::commit::CommitData;
use metadata::commit::headers::Headers;
use metadata::commit::trailers::OwnedTrailer;

use trailers::Trailers;

#[repr(transparent)]
pub(super) struct Commit(metadata::commit::CommitData<Oid, Oid>);

impl Commit {
    pub fn new<P, I, T>(
        tree: git2::Oid,
        parents: P,
        author: Author,
        committer: Author,
        headers: Headers,
        message: String,
        trailers: I,
    ) -> Self
    where
        P: IntoIterator<Item = Oid>,
        I: IntoIterator<Item = T>,
        OwnedTrailer: From<T>,
    {
        Self(CommitData::new(
            tree, parents, author, committer, headers, message, trailers,
        ))
    }
}

impl Commit {
    /// Read the [`Commit`] from the `repo` that is expected to be found at
    /// `oid`.
    pub fn read(repo: &git2::Repository, oid: Oid) -> Result<Self, error::Read> {
        let odb = repo.odb()?;
        let object = odb.read(oid)?;
        Ok(Commit::try_from(object.data())?)
    }

    /// Write the given [`Commit`] to the `repo`. The resulting `Oid`
    /// is the identifier for this commit.
    pub fn write(&self, repo: &git2::Repository) -> Result<Oid, error::Write> {
        let odb = repo.odb().map_err(error::Write::Odb)?;
        self.verify_for_write(&odb)?;
        Ok(odb.write(ObjectType::Commit, self.to_string().as_bytes())?)
    }

    fn verify_for_write(&self, odb: &git2::Odb) -> Result<(), error::Write> {
        for parent in self.0.parents() {
            verify_object(odb, &parent, ObjectType::Commit)?;
        }
        verify_object(odb, self.0.tree(), ObjectType::Tree)?;

        Ok(())
    }
}

fn verify_object(odb: &git2::Odb, oid: &Oid, expected: ObjectType) -> Result<(), error::Write> {
    use git2::{Error, ErrorClass, ErrorCode};

    let (_, kind) = odb
        .read_header(*oid)
        .map_err(|err| error::Write::OdbRead { oid: *oid, err })?;
    if kind != expected {
        Err(error::Write::NotCommit {
            oid: *oid,
            err: Error::new(
                ErrorCode::NotFound,
                ErrorClass::Object,
                format!("Object '{oid}' is not expected object type {expected}"),
            ),
        })
    } else {
        Ok(())
    }
}

pub mod error {
    use std::str;

    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Write {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("the parent '{oid}' provided is not a commit object")]
        NotCommit {
            oid: git2::Oid,
            #[source]
            err: git2::Error,
        },
        #[error("failed to access git odb")]
        Odb(#[source] git2::Error),
        #[error("failed to read '{oid}' from git odb")]
        OdbRead {
            oid: git2::Oid,
            #[source]
            err: git2::Error,
        },
    }

    #[derive(Debug, Error)]
    pub enum Read {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Parse(#[from] Parse),
    }

    #[derive(Debug, Error)]
    pub enum Parse {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Header(#[from] metadata::commit::headers::ParseError),
        #[error(transparent)]
        Utf8(#[from] str::Utf8Error),
    }
}

impl TryFrom<&[u8]> for Commit {
    type Error = error::Parse;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        Commit::from_str(str::from_utf8(data)?)
    }
}

impl FromStr for Commit {
    type Err = error::Parse;

    fn from_str(buffer: &str) -> Result<Self, Self::Err> {
        let (header, message) = buffer
            .split_once("\n\n")
            .ok_or(metadata::commit::headers::ParseError::InvalidFormat)?;

        let (tree, parents, author, committer, headers) =
            metadata::commit::headers::parse_commit_header(header)?;

        let trailers = Trailers::parse(message)?;

        let message = message
            .strip_suffix(&trailers.to_string(": "))
            .unwrap_or(message)
            .to_string();

        Ok(Self(CommitData::new(
            tree,
            parents,
            author,
            committer,
            headers,
            message,
            trailers.iter(),
        )))
    }
}

impl fmt::Display for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for Commit {
    type Target = CommitData<git2::Oid, git2::Oid>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
