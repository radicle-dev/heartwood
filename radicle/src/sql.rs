use std::ops::Deref;
use std::str::FromStr;

use sqlite as sql;
use sqlite::Value;

use crate::identity::Id;
use crate::node;
use crate::node::Address;

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
                message: Some("sql: invalid type for id".to_owned()),
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
                message: Some("sql: invalid type for node features".to_owned()),
            }),
        }
    }
}

impl TryFrom<&sql::Value> for Address {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::String(s) => Address::from_str(s.as_str()).map_err(|e| sql::Error {
                code: None,
                message: Some(e.to_string()),
            }),
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for address".to_owned()),
            }),
        }
    }
}

impl sql::BindableWithIndex for Address {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        self.to_string().bind(stmt, i)
    }
}
