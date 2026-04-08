pub mod headers;
pub mod trailers;

mod parse;
pub use parse::ParseError;

use core::fmt;
use std::str::{self, FromStr};

use headers::{Headers, Signature};
use trailers::{OwnedTrailer, Trailer};

use crate::author::Author;

/// A git commit in its object description form, i.e. the output of
/// `git cat-file` for a commit object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommitData<Tree, Parent> {
    tree: Tree,
    parents: Vec<Parent>,
    author: Author,
    committer: Author,
    headers: Headers,
    message: String,
    trailers: Vec<OwnedTrailer>,
}

impl<Tree, Parent> CommitData<Tree, Parent> {
    pub fn new<P, I, T>(
        tree: Tree,
        parents: P,
        author: Author,
        committer: Author,
        headers: Headers,
        message: String,
        trailers: I,
    ) -> Self
    where
        P: IntoIterator<Item = Parent>,
        I: IntoIterator<Item = T>,
        OwnedTrailer: From<T>,
    {
        let trailers = trailers.into_iter().map(OwnedTrailer::from).collect();
        let parents = parents.into_iter().collect();
        Self {
            tree,
            parents,
            author,
            committer,
            headers,
            message,
            trailers,
        }
    }

    /// The tree this commit points to.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The parents of this commit.
    pub fn parents(&self) -> impl Iterator<Item = Parent> + '_
    where
        Parent: Clone,
    {
        self.parents.iter().cloned()
    }

    /// The author of this commit, i.e. the header corresponding to `author`.
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// The committer of this commit, i.e. the header corresponding to
    /// `committer`.
    pub fn committer(&self) -> &Author {
        &self.committer
    }

    /// The message body of this commit.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The [`Signature`]s found in this commit, i.e. the headers corresponding
    /// to `gpgsig`.
    pub fn signatures(&self) -> impl Iterator<Item = Signature<'_>> + '_ {
        self.headers.signatures()
    }

    pub fn strip_signatures(mut self) -> Self {
        self.headers.strip_signatures();
        self
    }

    /// The [`Headers`] found in this commit.
    ///
    /// Note: these do not include `tree`, `parent`, `author`, and `committer`.
    pub fn headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter()
    }

    /// Iterate over the [`Headers`] values that match the provided `name`.
    pub fn values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers.values(name)
    }

    /// Push a header to the end of the headers section.
    pub fn push_header(&mut self, name: &str, value: &str) {
        self.headers.push(name, value.trim());
    }

    pub fn trailers(&self) -> impl Iterator<Item = &OwnedTrailer> {
        self.trailers.iter()
    }

    /// Convert the `CommitData::tree` into a value of type `U`. The
    /// conversion function `f` can be fallible.
    ///
    /// For example, `map_tree` can be used to turn raw tree data into
    /// an `Oid` by writing it to a repository.
    pub fn map_tree<U, E, F>(self, f: F) -> Result<CommitData<U, Parent>, E>
    where
        F: FnOnce(Tree) -> Result<U, E>,
    {
        Ok(CommitData {
            tree: f(self.tree)?,
            parents: self.parents,
            author: self.author,
            committer: self.committer,
            headers: self.headers,
            message: self.message,
            trailers: self.trailers,
        })
    }

    /// Convert the [`CommitData::parents`] into a vector containing
    /// values of type `U`. The conversion function `f` can be
    /// fallible.
    ///
    /// For example, this can be used to resolve the object identifiers
    /// to their respective full commits.
    pub fn map_parents<U, E, F>(self, f: F) -> Result<CommitData<Tree, U>, E>
    where
        F: FnMut(Parent) -> Result<U, E>,
    {
        Ok(CommitData {
            tree: self.tree,
            parents: self
                .parents
                .into_iter()
                .map(f)
                .collect::<Result<Vec<_>, _>>()?,
            author: self.author,
            committer: self.committer,
            headers: self.headers,
            message: self.message,
            trailers: self.trailers,
        })
    }
}

impl<Tree, Parent> CommitData<Tree, Parent>
where
    Tree: str::FromStr,
    Parent: str::FromStr,
    Tree::Err: std::error::Error + Send + Sync + 'static,
    Parent::Err: std::error::Error + Send + Sync + 'static,
{
    /// Parse a [`CommitData`] from its raw git object bytes.
    ///
    /// This is the inverse of the [`fmt::Display`] implementation. The bytes
    /// are expected to be valid UTF-8 and in the standard git commit object
    /// format produced by `git cat-file -p <commit>`.
    ///
    /// Trailers are detected by scanning the last paragraph of the message
    /// body (the section after the final blank line). If every non-empty line
    /// in that paragraph is a valid `Token: value` pair, those lines are
    /// parsed as trailers and stored separately; otherwise the whole body is
    /// kept as the message with no trailers.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let s = str::from_utf8(bytes).map_err(ParseError::Utf8)?;
        parse::parse(s)
    }
}

impl<Tree, Parent> FromStr for CommitData<Tree, Parent>
where
    Tree: str::FromStr,
    Parent: str::FromStr,
    Tree::Err: std::error::Error + Send + Sync + 'static,
    Parent::Err: std::error::Error + Send + Sync + 'static,
{
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse::parse(s)
    }
}

impl<Tree: fmt::Display, Parent: fmt::Display> fmt::Display for CommitData<Tree, Parent> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "tree {}", self.tree)?;
        for parent in self.parents.iter() {
            writeln!(f, "parent {parent}")?;
        }
        writeln!(f, "author {}", self.author)?;
        writeln!(f, "committer {}", self.committer)?;

        for (name, value) in self.headers.iter() {
            writeln!(f, "{name} {}", value.replace('\n', "\n "))?;
        }
        writeln!(f)?;
        write!(f, "{}", self.message.trim())?;
        writeln!(f)?;

        if !self.trailers.is_empty() {
            writeln!(f)?;
        }
        for trailer in self.trailers.iter() {
            writeln!(f, "{}", Trailer::from(trailer).display(": "))?;
        }
        Ok(())
    }
}
