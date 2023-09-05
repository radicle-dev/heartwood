use std::fmt::{self, Display};
use std::str::FromStr;

use localtime::LocalTime;
use serde::{Deserialize, Serialize};

use crate::git_ext::Oid;
use crate::prelude::*;

/// Timestamp used for COB operations.
pub type Timestamp = LocalTime;

/// Author.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Author {
    pub id: Did,
}

impl Author {
    pub fn new(id: impl Into<Did>) -> Self {
        Self { id: id.into() }
    }

    pub fn id(&self) -> &Did {
        &self.id
    }

    pub fn public_key(&self) -> &PublicKey {
        self.id.as_key()
    }
}

impl From<PublicKey> for Author {
    fn from(value: PublicKey) -> Self {
        Self::new(value)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ReactionError {
    #[error("invalid reaction")]
    InvalidReaction,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Serialize)]
#[serde(transparent)]
pub struct Reaction {
    emoji: char,
}

impl Reaction {
    /// Create a new reaction from an emoji.
    pub fn new(emoji: char) -> Result<Self, ReactionError> {
        let val = emoji as u32;
        let emoticons = 0x1F600..=0x1F64F;
        let misc = 0x1F300..=0x1F5FF; // Miscellaneous Symbols and Pictographs
        let dingbats = 0x2700..=0x27BF;
        let supp = 0x1F900..=0x1F9FF; // Supplemental Symbols and Pictographs
        let transport = 0x1F680..=0x1F6FF;

        if emoticons.contains(&val)
            || misc.contains(&val)
            || dingbats.contains(&val)
            || supp.contains(&val)
            || transport.contains(&val)
        {
            Ok(Self { emoji })
        } else {
            Err(ReactionError::InvalidReaction)
        }
    }

    /// Get the reaction emoji.
    pub fn emoji(&self) -> char {
        self.emoji
    }
}

impl<'de> Deserialize<'de> for Reaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReactionVisitor;

        impl<'de> serde::de::Visitor<'de> for ReactionVisitor {
            type Value = Reaction;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a reaction emoji")
            }

            fn visit_char<E>(self, v: char) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Reaction::new(v).map_err(|e| E::custom(e.to_string()))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Reaction::from_str(v).map_err(|e| E::custom(e.to_string()))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Reaction::from_str(&v).map_err(|e| E::custom(e.to_string()))
            }
        }

        deserializer.deserialize_char(ReactionVisitor)
    }
}

impl FromStr for Reaction {
    type Err = ReactionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        let first = chars.next().ok_or(ReactionError::InvalidReaction)?;

        // Reactions should not consist of more than a single emoji.
        if chars.next().is_some() {
            return Err(ReactionError::InvalidReaction);
        }
        Reaction::new(first)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LabelError {
    #[error("invalid tag name: `{0}`")]
    InvalidName(String),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Label(String);

impl Label {
    pub fn new(name: impl ToString) -> Result<Self, LabelError> {
        let name = name.to_string();

        if name.chars().any(|c| c.is_whitespace()) || name.is_empty() {
            return Err(LabelError::InvalidName(name));
        }
        Ok(Self(name))
    }

    pub fn name(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for Label {
    type Err = LabelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Label> for String {
    fn from(Label(name): Label) -> Self {
        name
    }
}

/// RGB color.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Color(u32);

#[derive(thiserror::Error, Debug)]
pub enum ColorConversionError {
    #[error("invalid format: expect '#rrggbb'")]
    InvalidFormat,
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:06x}", self.0)
    }
}

impl FromStr for Color {
    type Err = ColorConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.replace('#', "").to_lowercase();

        if hex.chars().count() != 6 {
            return Err(ColorConversionError::InvalidFormat);
        }

        match u32::from_str_radix(&hex, 16) {
            Ok(n) => Ok(Color(n)),
            Err(e) => Err(e.into()),
        }
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'a> Deserialize<'a> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'a>,
    {
        let color = String::deserialize(deserializer)?;
        Self::from_str(&color).map_err(serde::de::Error::custom)
    }
}

/// A URI.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Uri(String);

impl Uri {
    /// Get a string reference to the URI.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<Oid> for Uri {
    fn from(oid: Oid) -> Self {
        Uri(format!("git:{oid}"))
    }
}

impl TryFrom<&Uri> for Oid {
    type Error = Uri;

    fn try_from(value: &Uri) -> Result<Self, Self::Error> {
        if let Some(oid) = value.as_str().strip_prefix("git:") {
            let oid = oid.parse().map_err(|_| value.clone())?;

            return Ok(oid);
        }
        Err(value.clone())
    }
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Uri {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.chars().all(|c| c.is_ascii()) {
            return Err(s.to_owned());
        }
        if !s.contains(':') {
            return Err(s.to_owned());
        }
        Ok(Self(s.to_owned()))
    }
}

/// The result of an authorization check on an COB action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// Action is allowed.
    Allow,
    /// Action is denied.
    Deny,
    /// Authorization cannot be determined due to missing object, eg. due to redaction.
    Unknown,
}

impl From<bool> for Authorization {
    fn from(value: bool) -> Self {
        if value {
            Self::Allow
        } else {
            Self::Deny
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;

    #[test]
    fn test_color() {
        let c = Color::from_str("#ffccaa").unwrap();
        assert_eq!(c.to_string(), "#ffccaa".to_owned());
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"#ffccaa\"".to_owned());
        assert_eq!(serde_json::from_str::<'_, Color>("\"#ffccaa\"").unwrap(), c);

        let c = Color::from_str("#0000aa").unwrap();
        assert_eq!(c.to_string(), "#0000aa".to_owned());

        let c = Color::from_str("#aa0000").unwrap();
        assert_eq!(c.to_string(), "#aa0000".to_owned());

        let c = Color::from_str("#00aa00").unwrap();
        assert_eq!(c.to_string(), "#00aa00".to_owned());

        Color::from_str("#aa00").unwrap_err();
        Color::from_str("#abc").unwrap_err();
    }
}
