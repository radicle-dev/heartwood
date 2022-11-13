#![allow(clippy::large_enum_variant)]
use std::str::FromStr;
use std::sync::Arc;

pub use automerge::{ScalarValue, Value};

use crate::git;
use crate::prelude::*;

/// Implemented by types that can be converted from a [`Value`].
pub trait FromValue<'a>: Sized {
    fn from_value(val: Value<'a>) -> Result<Self, ValueError>;
}

#[derive(thiserror::Error, Debug)]
pub enum ValueError {
    #[error("invalid type")]
    InvalidType,
    #[error("invalid value: `{0}`")]
    InvalidValue(String),
    #[error("value error: {0}")]
    Other(Arc<dyn std::error::Error + Send + Sync>),
}

impl<'a, T> FromValue<'a> for Option<T>
where
    T: FromValue<'a>,
{
    fn from_value(val: Value<'a>) -> Result<Option<T>, ValueError> {
        match val {
            Value::Scalar(s) if s.is_null() => Ok(None),
            _ => Ok(Some(T::from_value(val)?)),
        }
    }
}

impl<'a> FromValue<'a> for NodeId {
    fn from_value(val: Value<'a>) -> Result<NodeId, ValueError> {
        let peer = String::from_value(val)?;
        let peer = NodeId::from_str(&peer).map_err(|e| ValueError::Other(Arc::new(e)))?;

        Ok(peer)
    }
}

impl<'a> FromValue<'a> for uuid::Uuid {
    fn from_value(val: Value<'a>) -> Result<uuid::Uuid, ValueError> {
        let uuid = String::from_value(val)?;
        let uuid = uuid::Uuid::from_str(&uuid).map_err(|e| ValueError::Other(Arc::new(e)))?;

        Ok(uuid)
    }
}

impl<'a> FromValue<'a> for Id {
    fn from_value(val: Value<'a>) -> Result<Id, ValueError> {
        let id = String::from_value(val)?;
        let id = Id::from_str(&id).map_err(|e| ValueError::Other(Arc::new(e)))?;

        Ok(id)
    }
}

impl<'a> FromValue<'a> for git::Oid {
    fn from_value(val: Value<'a>) -> Result<git::Oid, ValueError> {
        let oid = String::from_value(val)?;
        let oid = git::Oid::from_str(&oid).map_err(|e| ValueError::Other(Arc::new(e)))?;

        Ok(oid)
    }
}

impl<'a> FromValue<'a> for String {
    fn from_value(val: Value) -> Result<String, ValueError> {
        val.into_string().map_err(|_| ValueError::InvalidType)
    }
}
