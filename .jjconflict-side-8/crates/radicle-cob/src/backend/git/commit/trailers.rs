use std::{borrow::Cow, fmt, fmt::Write, str::FromStr};

use git2::{MessageTrailersStrs, MessageTrailersStrsIterator};

use metadata::commit::trailers::Separator;

/// A Git commit's set of trailers that are left in the commit's
/// message.
///
/// Trailers are key/value pairs in the last paragraph of a message,
/// not including any patches or conflicts that may be present.
///
/// # Usage
///
/// To construct `Trailers`, you can use [`Trailers::parse`] or its
/// `FromStr` implementation.
///
/// To iterate over the trailers, you can use [`Trailers::iter`].
///
/// To render the trailers to a `String`, you can use
/// [`Trailers::to_string`] or its `Display` implementation (note that
/// it will default to using `": "` as the separator.
///
/// # Examples
///
/// ```text
/// Add new functionality
///
/// Making code better with new functionality.
///
/// X-Signed-Off-By: Alex Sellier
/// X-Co-Authored-By: Fintan Halpenny
/// ```
///
/// The trailers in the above example are:
///
/// ```text
/// X-Signed-Off-By: Alex Sellier
/// X-Co-Authored-By: Fintan Halpenny
/// ```
pub struct Trailers {
    inner: MessageTrailersStrs,
}

impl Trailers {
    pub fn parse(message: &str) -> Result<Self, git2::Error> {
        Ok(Self {
            inner: git2::message_trailers_strs(message)?,
        })
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.inner.iter(),
        }
    }

    pub fn to_string<'a, S>(&self, sep: S) -> String
    where
        S: Separator<'a>,
    {
        let mut buf = String::new();
        for (i, trailer) in self.iter().enumerate() {
            if i > 0 {
                writeln!(buf).ok();
            }

            write!(buf, "{}", trailer.display(sep.sep_for(&trailer.token))).ok();
        }
        writeln!(buf).ok();
        buf
    }
}

impl fmt::Display for Trailers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string(": "))
    }
}

impl FromStr for Trailers {
    type Err = git2::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

pub struct Iter<'a> {
    inner: MessageTrailersStrsIterator<'a>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = metadata::commit::trailers::Trailer<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (token, value) = self.inner.next()?;
        Some(metadata::commit::trailers::Trailer {
            token: {
                // This code used to live in the same module with `Token`,
                // but was separated because it depends on `git2`.
                // We have no way of directly constructing a `Token`, anymore
                // but `git2` still guarantees that the trailer is well-formed.
                metadata::commit::trailers::Token::try_from(token)
                    .expect("token from `git2` must be valid")
            },
            value: Cow::Borrowed(value),
        })
    }
}
