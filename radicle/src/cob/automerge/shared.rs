#![allow(clippy::large_enum_variant)]
use std::borrow::Borrow;
use std::collections::HashMap;
use std::str::FromStr;

use automerge::transaction::Transactable;
use automerge::{AutomergeError, ObjType, ScalarValue};
use serde::{Deserialize, Serialize};

use crate::cob::automerge::doc::{Document, DocumentError};
use crate::cob::automerge::store::Error;
use crate::cob::automerge::value::{FromValue, Value, ValueError};
use crate::cob::common::*;
use crate::cob::{History, TypeName};

/// A type that can be materialized from an event history.
/// All collaborative objects implement this trait.
pub trait FromHistory: Sized {
    /// The object type name.
    fn type_name() -> &'static TypeName;
    /// Create an object from a history.
    fn from_history(history: &History) -> Result<Self, Error>;
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

impl From<&Author> for ScalarValue {
    fn from(author: &Author) -> Self {
        ScalarValue::from(author.id.to_human())
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

impl From<Timestamp> for ScalarValue {
    fn from(ts: Timestamp) -> Self {
        ScalarValue::Timestamp(ts.as_secs() as i64)
    }
}

impl<'a> FromValue<'a> for Timestamp {
    fn from_value(val: Value<'a>) -> Result<Self, ValueError> {
        if let Value::Scalar(scalar) = &val {
            if let ScalarValue::Timestamp(ts) = scalar.borrow() {
                return Ok(Self::new(*ts as u64));
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
