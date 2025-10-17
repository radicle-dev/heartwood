use core::fmt;
use std::borrow::Cow;

const BEGIN_SSH: &str = "-----BEGIN SSH SIGNATURE-----\n";
const BEGIN_PGP: &str = "-----BEGIN PGP SIGNATURE-----\n";

/// A collection of headers stored in [`super::CommitData`].
///
/// Note: these do not include `tree`, `parent`, `author`, and `committer`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Headers(pub(super) Vec<(String, String)>);

/// A `gpgsig` signature stored in [`super::CommitData`].
#[derive(Debug)]
pub enum Signature<'a> {
    /// A PGP signature, i.e. starts with `-----BEGIN PGP SIGNATURE-----`.
    Pgp(Cow<'a, str>),
    /// A SSH signature, i.e. starts with `-----BEGIN SSH SIGNATURE-----`.
    Ssh(Cow<'a, str>),
}

impl<'a> Signature<'a> {
    fn from_str(s: &'a str) -> Result<Self, UnknownScheme> {
        if s.starts_with(BEGIN_SSH) {
            Ok(Signature::Ssh(Cow::Borrowed(s)))
        } else if s.starts_with(BEGIN_PGP) {
            Ok(Signature::Pgp(Cow::Borrowed(s)))
        } else {
            Err(UnknownScheme)
        }
    }
}

impl fmt::Display for Signature<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signature::Pgp(pgp) => f.write_str(pgp.as_ref()),
            Signature::Ssh(ssh) => f.write_str(ssh.as_ref()),
        }
    }
}

pub struct UnknownScheme;

impl Headers {
    pub fn new() -> Self {
        Headers(Vec::new())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.iter()
            .filter_map(move |(k, v)| (k == name).then_some(v))
    }

    pub fn signatures(&self) -> impl Iterator<Item = Signature<'_>> + '_ {
        self.0.iter().filter_map(|(k, v)| {
            if k == "gpgsig" {
                Signature::from_str(v).ok()
            } else {
                None
            }
        })
    }

    /// Push a header to the end of the headers section.
    pub fn push(&mut self, name: &str, value: &str) {
        self.0.push((name.to_owned(), value.trim().to_owned()));
    }

    pub(crate) fn strip_signatures(&mut self) {
        self.0.retain(|(key, _)| key != "gpgsig");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("missing tree")]
    MissingTree,
    #[error("invalid tree")]
    InvalidTree,
    #[error("invalid format")]
    InvalidFormat,
    #[error("invalid parent")]
    InvalidParent,
    #[error("invalid header")]
    InvalidHeader,
    #[error("invalid author")]
    InvalidAuthor,
    #[error("missing author")]
    MissingAuthor,
    #[error("invalid committer")]
    InvalidCommitter,
    #[error("missing committer")]
    MissingCommitter,
}

pub fn parse_commit_header<
    Tree: std::str::FromStr,
    Parent: std::str::FromStr,
    Signature: std::str::FromStr,
>(
    header: &str,
) -> Result<(Tree, Vec<Parent>, Signature, Signature, Headers), ParseError> {
    let mut lines = header.lines();

    let tree = match lines.next() {
        Some(tree) => tree
            .strip_prefix("tree ")
            .map(Tree::from_str)
            .transpose()
            .map_err(|_| ParseError::InvalidTree)?
            .ok_or(ParseError::MissingTree)?,
        None => return Err(ParseError::MissingTree),
    };

    let mut parents = Vec::new();
    let mut author: Option<Signature> = None;
    let mut committer: Option<Signature> = None;
    let mut headers = Headers::new();

    for line in lines {
        // Check if a signature is still being parsed
        if let Some(rest) = line.strip_prefix(' ') {
            let value: &mut String = headers
                .0
                .last_mut()
                .map(|(_, v)| v)
                .ok_or(ParseError::InvalidFormat)?;
            value.push('\n');
            value.push_str(rest);
            continue;
        }

        if let Some((name, value)) = line.split_once(' ') {
            match name {
                "parent" => parents.push(
                    value
                        .parse::<Parent>()
                        .map_err(|_| ParseError::InvalidParent)?,
                ),
                "author" => {
                    author = Some(
                        value
                            .parse::<Signature>()
                            .map_err(|_| ParseError::InvalidAuthor)?,
                    )
                }
                "committer" => {
                    committer = Some(
                        value
                            .parse::<Signature>()
                            .map_err(|_| ParseError::InvalidCommitter)?,
                    )
                }
                _ => headers.push(name, value),
            }
            continue;
        }
    }

    Ok((
        tree,
        parents,
        author.ok_or(ParseError::MissingAuthor)?,
        committer.ok_or(ParseError::MissingCommitter)?,
        headers,
    ))
}
