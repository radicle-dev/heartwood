#![allow(clippy::large_enum_variant)]
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use automerge::transaction::Transactable;
use automerge::{AutomergeError, ObjType, ScalarValue};
use serde::{Deserialize, Serialize};

use crate::cob::doc::{Document, DocumentError};
use crate::cob::value::{FromValue, Value, ValueError};
use crate::prelude::*;

/// A discussion thread.
pub type Discussion = Vec<Comment<Replies>>;

#[derive(thiserror::Error, Debug)]
pub enum ReactionError {
    #[error("invalid reaction")]
    InvalidReaction,
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Reaction {
    pub emoji: char,
}

impl Reaction {
    pub fn new(emoji: char) -> Result<Self, ReactionError> {
        if emoji.is_whitespace() || emoji.is_ascii() || emoji.is_alphanumeric() {
            return Err(ReactionError::InvalidReaction);
        }
        Ok(Self { emoji })
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
    #[error("invalid label name: `{0}`")]
    InvalidName(String),
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Label(String);

impl Label {
    pub fn new(name: impl Into<String>) -> Result<Self, LabelError> {
        let name = name.into();

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

impl<'a> FromValue<'a> for Color {
    fn from_value(val: Value<'a>) -> Result<Self, ValueError> {
        let color = String::from_value(val)?;
        let color = Self::from_str(&color).map_err(|_| ValueError::InvalidValue(color))?;

        Ok(color)
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

/// Author.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Author {
    pub id: NodeId,
}

impl Author {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &NodeId {
        &self.id
    }
}

impl From<&Author> for ScalarValue {
    fn from(author: &Author) -> Self {
        ScalarValue::from(author.id.to_human())
    }
}

/// Local id of a comment in an issue.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct CommentId {
    /// Represents the index of the comment in the thread,
    /// with `0` being the top-level comment.
    ix: usize,
}

impl CommentId {
    /// Root comment.
    pub const fn root() -> Self {
        Self { ix: 0 }
    }
}

impl From<usize> for CommentId {
    fn from(ix: usize) -> Self {
        Self { ix }
    }
}

impl From<CommentId> for usize {
    fn from(id: CommentId) -> Self {
        id.ix
    }
}

/// Comment replies.
pub type Replies = Vec<Comment>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment<R = ()> {
    pub author: Author,
    pub body: String,
    pub reactions: HashMap<Reaction, usize>,
    pub replies: R,
    pub timestamp: Timestamp,
}

impl<R: Default> Comment<R> {
    pub fn new(author: Author, body: String, timestamp: Timestamp) -> Self {
        Self {
            author,
            body,
            reactions: HashMap::default(),
            replies: R::default(),
            timestamp,
        }
    }
}

impl Comment<()> {
    pub(super) fn put(
        &self,
        tx: &mut automerge::transaction::Transaction,
        id: &automerge::ObjId,
    ) -> Result<(), AutomergeError> {
        let comment_id = tx.put_object(id, "comment", ObjType::Map)?;

        assert!(
            self.reactions.is_empty(),
            "Cannot put comment with non-empty reactions"
        );

        tx.put(&comment_id, "body", self.body.trim())?;
        tx.put(&comment_id, "author", self.author.id().to_string())?;
        tx.put(&comment_id, "timestamp", self.timestamp)?;
        tx.put_object(&comment_id, "reactions", ObjType::Map)?;

        Ok(())
    }
}

impl Comment<Replies> {
    pub(super) fn put(
        &self,
        tx: &mut automerge::transaction::Transaction,
        id: &automerge::ObjId,
    ) -> Result<(), AutomergeError> {
        let comment_id = tx.put_object(id, "comment", ObjType::Map)?;

        assert!(
            self.reactions.is_empty(),
            "Cannot put comment with non-empty reactions"
        );
        assert!(
            self.replies.is_empty(),
            "Cannot put comment with non-empty replies"
        );

        tx.put(&comment_id, "body", self.body.trim())?;
        tx.put(&comment_id, "author", self.author.id().to_string())?;
        tx.put(&comment_id, "timestamp", self.timestamp)?;
        tx.put_object(&comment_id, "reactions", ObjType::Map)?;
        tx.put_object(&comment_id, "replies", ObjType::List)?;

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp {
    seconds: u64,
}

impl Timestamp {
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }

    pub fn now() -> Self {
        #[allow(clippy::unwrap_used)] // Safe because Unix was already invented!
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Self {
            seconds: duration.as_secs(),
        }
    }

    pub fn as_secs(&self) -> u64 {
        self.seconds
    }
}

impl From<Timestamp> for ScalarValue {
    fn from(ts: Timestamp) -> Self {
        ScalarValue::Timestamp(ts.seconds as i64)
    }
}

impl<'a> FromValue<'a> for Timestamp {
    fn from_value(val: Value<'a>) -> Result<Self, ValueError> {
        if let Value::Scalar(scalar) = &val {
            if let ScalarValue::Timestamp(ts) = scalar.borrow() {
                return Ok(Self {
                    seconds: *ts as u64,
                });
            }
        }
        Err(ValueError::InvalidValue(val.to_string()))
    }
}

pub mod lookup {
    use super::{Author, Comment, HashMap, Reaction, Replies};
    use super::{Document, DocumentError};

    pub fn comment(doc: Document, obj_id: &automerge::ObjId) -> Result<Comment<()>, DocumentError> {
        let author = doc.get(obj_id, "author").map(Author::new)?;
        let body = doc.get(obj_id, "body")?;
        let timestamp = doc.get(obj_id, "timestamp")?;
        let reactions: HashMap<Reaction, usize> = doc.map(obj_id, "reactions", |v| *v += 1)?;

        Ok(Comment {
            author,
            body,
            reactions,
            replies: (),
            timestamp,
        })
    }

    pub fn thread(
        doc: Document,
        obj_id: &automerge::ObjId,
    ) -> Result<Comment<Replies>, DocumentError> {
        let comment = self::comment(doc, obj_id)?;
        let replies = doc.list(obj_id, "replies", self::comment)?;

        Ok(Comment {
            author: comment.author,
            body: comment.body,
            reactions: comment.reactions,
            replies,
            timestamp: comment.timestamp,
        })
    }
}

#[cfg(test)]
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
