use std::{borrow::Cow, fmt, ops::Deref};

pub trait Separator<'a> {
    fn sep_for(&self, token: &Token) -> &'a str;
}

impl<'a> Separator<'a> for &'a str {
    fn sep_for(&self, _: &Token) -> &'a str {
        self
    }
}

impl<'a, F> Separator<'a> for F
where
    F: Fn(&Token) -> &'a str,
{
    fn sep_for(&self, token: &Token) -> &'a str {
        self(token)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Token<'a>(&'a str);

impl Deref for Token<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> TryFrom<&'a str> for Token<'a> {
    type Error = &'static str;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        let is_token = s.chars().all(|c| c.is_alphanumeric() || c == '-');
        if is_token {
            Ok(Token(s))
        } else {
            Err("token contains invalid characters")
        }
    }
}

pub struct Display<'a> {
    trailer: &'a Trailer<'a>,
    separator: &'a str,
}

impl fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.trailer.token.deref(),
            self.separator,
            self.trailer.value,
        )
    }
}

/// A trailer is a key/value pair found in the last paragraph of a Git
/// commit message, not including any patches or conflicts that may be
/// present.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Trailer<'a> {
    pub token: Token<'a>,
    pub value: Cow<'a, str>,
}

impl<'a> Trailer<'a> {
    pub fn display(&'a self, separator: &'a str) -> Display<'a> {
        Display {
            trailer: self,
            separator,
        }
    }

    pub fn to_owned(&self) -> OwnedTrailer {
        OwnedTrailer::from(self)
    }
}

/// A version of the [`Trailer`] which owns its token and
/// value. Useful for when you need to carry trailers around in a long
/// lived data structure.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnedTrailer {
    pub token: OwnedToken,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnedToken(String);

impl Deref for OwnedToken {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> From<&Trailer<'a>> for OwnedTrailer {
    fn from(t: &Trailer<'a>) -> Self {
        OwnedTrailer {
            token: OwnedToken(t.token.0.to_string()),
            value: t.value.to_string(),
        }
    }
}

impl<'a> From<Trailer<'a>> for OwnedTrailer {
    fn from(t: Trailer<'a>) -> Self {
        (&t).into()
    }
}

impl<'a> From<&'a OwnedTrailer> for Trailer<'a> {
    fn from(t: &'a OwnedTrailer) -> Self {
        Trailer {
            token: Token(t.token.0.as_str()),
            value: Cow::from(&t.value),
        }
    }
}
