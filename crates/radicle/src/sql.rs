use std::ops::Deref;
use std::str::FromStr;

use sqlite as sql;
use sqlite::Value;

use crate::identity::RepoId;
use crate::node;
use crate::node::{Address, UserAgent};

/// Run an SQL query inside a transaction.
/// Commits the transaction on success, and rolls back on error.
pub fn transaction<T, E: From<sql::Error>>(
    db: &sql::Connection,
    query: impl FnOnce(&sql::Connection) -> Result<T, E>,
) -> Result<T, E> {
    db.execute("BEGIN")?;

    match query(db) {
        Ok(result) => {
            db.execute("COMMIT")?;
            Ok(result)
        }
        Err(err) => {
            db.execute("ROLLBACK")?;
            Err(err)
        }
    }
}

impl TryFrom<&Value> for RepoId {
    type Error = sql::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(id) => RepoId::from_urn(id).map_err(|e| sql::Error {
                code: None,
                message: Some(e.to_string()),
            }),
            _ => Err(sql::Error {
                code: None,
                message: Some(format!("sql: invalid type `{:?}` for id", value.kind())),
            }),
        }
    }
}

impl sqlite::BindableWithIndex for &RepoId {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        self.urn().as_str().bind(stmt, i)
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
                message: Some(format!(
                    "sql: invalid type `{:?}` for node features",
                    value.kind()
                )),
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
                message: Some(format!(
                    "sql: invalid type `{:?}` for address",
                    value.kind()
                )),
            }),
        }
    }
}

impl sql::BindableWithIndex for &Address {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        self.to_string().bind(stmt, i)
    }
}

impl TryFrom<&Value> for UserAgent {
    type Error = sql::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(ua) => UserAgent::from_str(ua).map_err(|e| sql::Error {
                code: None,
                message: Some(e.to_string()),
            }),
            _ => Err(sql::Error {
                code: None,
                message: Some(format!(
                    "sql: invalid type `{:?}` for user-agent",
                    value.kind()
                )),
            }),
        }
    }
}
