use std::ops::Deref;
use std::str::FromStr;

use sqlite as sql;
use sqlite::Value;

use crate::identity::Id;
use crate::node;

impl TryFrom<&Value> for Id {
    type Error = sql::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(id) => Id::from_str(id).map_err(|e| sql::Error {
                code: None,
                message: Some(e.to_string()),
            }),
            _ => Err(sql::Error {
                code: None,
                message: None,
            }),
        }
    }
}

impl sqlite::BindableWithIndex for &Id {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        self.to_human().as_str().bind(stmt, i)
    }
}

impl sql::BindableWithIndex for node::Features {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        (*self.deref() as i64).bind(stmt, i)
    }
}

impl TryFrom<&Value> for node::Features {
    type Error = sql::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Integer(bits) => Ok(node::Features::from(*bits as u64)),
            _ => Err(sql::Error {
                code: None,
                message: None,
            }),
        }
    }
}
