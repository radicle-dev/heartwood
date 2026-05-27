#[cfg(not(creusot))]
use std::fmt;
#[cfg(not(creusot))]
use std::fmt::Display;
#[cfg(not(creusot))]
use std::ops::{Deref, Range};
#[cfg(not(creusot))]
use std::path::PathBuf;
#[cfg(not(creusot))]
use std::str::FromStr;

#[cfg(not(creusot))]
use base64::prelude::{BASE64_STANDARD, Engine};
use localtime::LocalTime;
#[cfg(not(creusot))]
use serde::{Deserialize, Serialize};
#[cfg(not(creusot))]
use thiserror::Error;

#[cfg(not(creusot))]
use crate::git::Oid;
use crate::prelude::{Did, PublicKey};

/// Timestamp used for COB operations.
#[derive(Debug, Clone, Copy, Eq, PartialOrd, Ord)]
#[cfg_attr(not(creusot), derive(PartialEq))]
#[cfg_attr(creusot, derive(creusot_std::prelude::PartialEq))]
pub struct Timestamp(LocalTime);

#[cfg(creusot)]
impl creusot_std::model::View for Timestamp {
    type ViewTy = u64;

    #[creusot_std::prelude::logic(opaque)]
    fn view(self) -> Self::ViewTy {
        dead
    }
}

#[cfg(creusot)]
impl creusot_std::prelude::DeepModel for Timestamp {
    type DeepModelTy = u64;

    #[creusot_std::prelude::logic(open, inline)]
    fn deep_model(self) -> Self::DeepModelTy {
        use creusot_std::model::View as _;
        self.view()
    }
}

#[cfg(not(creusot))]
impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0.as_millis())
    }
}

#[cfg(not(creusot))]
impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        u128::deserialize(deserializer)
            .map(LocalTime::from_millis)
            .map(Self)
    }
}

impl Timestamp {
    pub fn from_secs(secs: u64) -> Self {
        Self(LocalTime::from_secs(secs))
    }
}

#[cfg(not(creusot))]
impl From<LocalTime> for Timestamp {
    fn from(time: LocalTime) -> Self {
        Self(time)
    }
}

#[cfg(not(creusot))]
impl From<Timestamp> for LocalTime {
    fn from(time: Timestamp) -> Self {
        time.0
    }
}

#[cfg(not(creusot))]
impl Deref for Timestamp {
    type Target = LocalTime;

    #[cfg_attr(creusot, creusot_std::prelude::check(ghost))]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
#[cfg(not(creusot))]
pub enum TitleError {
    #[error("empty title")]
    EmptyTitle,
    #[error("invalid characters in title")]
    InvalidTitle,
}

/// A `Title` is used for messages that are included in collaborative objects,
/// such as patches, issues, and identity changes.
///
/// A `Title`:
///   - Must not be empty
///   - Must not contain `\n` or `\r` characters
///   - Will be trimmed of any preceding or following whitespace
//#[derive(Display, PartialEq, Eq, Clone, Debug)]
#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(not(creusot), derive(Deserialize, Serialize))]
//#[display(inner)]
pub struct Title(String);

#[cfg(creusot)]
impl creusot_std::model::View for Title {
    type ViewTy = creusot_std::prelude::Seq<char>;

    #[creusot_std::prelude::logic(opaque)]
    fn view(self) -> Self::ViewTy {
        dead
    }
}

#[cfg(creusot)]
impl creusot_std::prelude::DeepModel for Title {
    type DeepModelTy = creusot_std::prelude::Seq<char>;

    #[creusot_std::prelude::logic]
    fn deep_model(self) -> Self::DeepModelTy {
        use creusot_std::model::View as _;
        self.view()
    }
}

#[cfg(not(creusot))]
impl Title {
    /// # Errors
    ///
    /// [`TitleError::EmptyTitle`]: the provided `title` was empty
    /// [`TitleError::InvalidTitle`]: the provided `title` contained invalid
    /// characters
    pub fn new(title: &str) -> Result<Self, TitleError> {
        if title.contains('\n') || title.contains('\r') {
            return Err(TitleError::InvalidTitle);
        }

        let title = title.trim();
        if title.is_empty() {
            Err(TitleError::EmptyTitle)
        } else {
            Ok(Self(title.into()))
        }
    }
}

#[cfg(not(creusot))]
impl AsRef<str> for Title {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(not(creusot))]
impl FromStr for Title {
    type Err = TitleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[cfg(not(creusot))]
impl TryFrom<String> for Title {
    type Error = TitleError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

/// Author.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(not(creusot), derive(Serialize, Deserialize))]
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

#[cfg(not(creusot))]
impl Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.id.fmt(f)
    }
}

impl From<PublicKey> for Author {
    fn from(value: PublicKey) -> Self {
        Self::new(value)
    }
}

#[derive(thiserror::Error, Debug)]
#[cfg(not(creusot))]
pub enum ReactionError {
    #[error("invalid reaction")]
    InvalidReaction,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Serialize)]
#[serde(transparent)]
#[cfg(not(creusot))]
pub struct Reaction {
    emoji: char,
}

#[cfg(not(creusot))]
impl Reaction {
    /// Create a new reaction from an emoji.
    pub fn new(emoji: char) -> Result<Self, ReactionError> {
        let val = emoji as u32;
        let emoticons = 0x1F600..=0x1F64F;
        let hearts = 0x1FA75..=0x1FA77;
        let body_update = 0x1FAC0..=0x1FAC6;
        let emoticons_update = 0x1FAE0..=0x1FAF8;
        let misc = 0x1F300..=0x1F5FF; // Miscellaneous Symbols and Pictographs
        let dingbats = 0x2700..=0x27BF;
        let supp = 0x1F900..=0x1F9FF; // Supplemental Symbols and Pictographs
        let transport = 0x1F680..=0x1F6FF;

        if emoticons.contains(&val)
            || hearts.contains(&val)
            || body_update.contains(&val)
            || emoticons_update.contains(&val)
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

#[cfg(not(creusot))]
impl<'de> Deserialize<'de> for Reaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReactionVisitor;

        impl serde::de::Visitor<'_> for ReactionVisitor {
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

#[cfg(not(creusot))]
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
#[cfg(not(creusot))]
pub enum LabelError {
    #[error("invalid tag name: `{0}`")]
    InvalidName(String),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg(not(creusot))]
pub struct Label(String);

#[cfg(not(creusot))]
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

#[cfg(not(creusot))]
impl FromStr for Label {
    type Err = LabelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[cfg(not(creusot))]
impl Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(not(creusot))]
impl From<Label> for String {
    fn from(Label(name): Label) -> Self {
        name
    }
}

/// RGB color.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
#[cfg(not(creusot))]
pub struct Color(u32);

#[derive(thiserror::Error, Debug)]
#[cfg(not(creusot))]
pub enum ColorConversionError {
    #[error("invalid format: expect '#rrggbb'")]
    InvalidFormat,
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

#[cfg(not(creusot))]
impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:06x}", self.0)
    }
}

#[cfg(not(creusot))]
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

#[cfg(not(creusot))]
impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

#[cfg(not(creusot))]
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
#[cfg(not(creusot))]
pub struct Uri(String);

#[cfg(not(creusot))]
impl Uri {
    /// Get a string reference to the URI.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(not(creusot))]
impl From<Oid> for Uri {
    fn from(oid: Oid) -> Self {
        Uri(format!("git:{oid}"))
    }
}

#[cfg(not(creusot))]
impl TryFrom<&Uri> for crate::git::raw::Oid {
    type Error = Uri;

    fn try_from(value: &Uri) -> Result<Self, Self::Error> {
        if let Some(oid) = value.as_str().strip_prefix("git:") {
            let oid = oid.parse().map_err(|_| value.clone())?;

            return Ok(oid);
        }
        Err(value.clone())
    }
}

#[cfg(not(creusot))]
impl TryFrom<&Uri> for crate::git::Oid {
    type Error = Uri;

    fn try_from(value: &Uri) -> Result<Self, Self::Error> {
        crate::git::raw::Oid::try_from(value).map(crate::git::Oid::from)
    }
}

#[cfg(not(creusot))]
impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(not(creusot))]
impl std::str::FromStr for Uri {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.is_ascii() {
            return Err(s.to_owned());
        }
        if !s.contains(':') {
            return Err(s.to_owned());
        }
        Ok(Self(s.to_owned()))
    }
}

/// A `data:` URI.
#[derive(Debug, Clone)]
#[cfg(not(creusot))]
pub struct DataUri(Vec<u8>);

#[cfg(not(creusot))]
impl From<DataUri> for Vec<u8> {
    fn from(value: DataUri) -> Self {
        value.0
    }
}

#[cfg(not(creusot))]
impl TryFrom<&Uri> for DataUri {
    type Error = Uri;

    fn try_from(value: &Uri) -> Result<Self, Self::Error> {
        if let Some(data_uri) = value.as_str().strip_prefix("data:") {
            let (_, uri_data) = data_uri.split_once(',').ok_or(value.clone())?;
            let uri_data = BASE64_STANDARD
                .decode(uri_data)
                .map_err(|_| value.clone())?;

            return Ok(DataUri(uri_data));
        }
        Err(value.clone())
    }
}

/// The result of an authorization check on an COB action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(not(creusot))]
pub enum Authorization {
    /// Action is allowed.
    Allow,
    /// Action is denied.
    Deny,
    /// Authorization cannot be determined due to missing object, eg. due to redaction.
    Unknown,
}

#[cfg(not(creusot))]
impl From<bool> for Authorization {
    fn from(value: bool) -> Self {
        if value { Self::Allow } else { Self::Deny }
    }
}

/// Describes a code location that can be used for comments on
/// patches, issues, and diffs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg(not(creusot))]
pub struct CodeLocation {
    /// [`Oid`] of the Git commit.
    pub commit: Oid,
    /// Path of file.
    pub path: PathBuf,
    /// Line range on old file. `None` for added files.
    pub old: Option<CodeRange>,
    /// Line range on new file. `None` for deleted files.
    pub new: Option<CodeRange>,
}

/// Code range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
#[cfg(not(creusot))]
pub enum CodeRange {
    /// One or more lines.
    Lines { range: Range<usize> },
    /// Character range within a line.
    Chars { line: usize, range: Range<usize> },
}

#[cfg(not(creusot))]
impl std::cmp::PartialOrd for CodeRange {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(not(creusot))]
impl std::cmp::Ord for CodeRange {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (CodeRange::Lines { .. }, CodeRange::Chars { .. }) => std::cmp::Ordering::Less,
            (CodeRange::Chars { .. }, CodeRange::Lines { .. }) => std::cmp::Ordering::Greater,

            (CodeRange::Lines { range: a }, CodeRange::Lines { range: b }) => {
                a.clone().cmp(b.clone())
            }
            (
                CodeRange::Chars {
                    line: l1,
                    range: r1,
                },
                CodeRange::Chars {
                    line: l2,
                    range: r2,
                },
            ) => l1.cmp(l2).then(r1.clone().cmp(r2.clone())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn test_title() {
        assert_eq!(Title::new(""), Err(TitleError::EmptyTitle));
        assert_eq!(Title::new(" "), Err(TitleError::EmptyTitle));
        assert_eq!(Title::new("\t"), Err(TitleError::EmptyTitle));
        assert_eq!(Title::new("foo\nbar"), Err(TitleError::InvalidTitle));
        assert_eq!(Title::new("foobar\n"), Err(TitleError::InvalidTitle));
        assert_eq!(Title::new(" valid title ").unwrap().0, "valid title");
    }

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

    #[test]
    fn test_emojis() {
        let emojis = emojis::Group::SmileysAndEmotion
            .emojis()
            .chain(emojis::Group::PeopleAndBody.emojis())
            .filter_map(|emoji| {
                if emoji.as_str().chars().count() == 1 {
                    Some(emoji.as_str().chars().next().unwrap())
                } else {
                    None
                }
            });
        let mut failed = BTreeSet::new();
        for emoji in emojis {
            if Reaction::new(emoji).is_err() {
                failed.insert(emoji);
            }
        }
        if !failed.is_empty() {
            for emoji in failed {
                eprintln!(
                    "cannot construct Reaction for '{emoji}' {:#04X}",
                    (emoji as u32)
                );
            }
            panic!("Failed to construct Reaction for these emojis");
        }
    }
}
