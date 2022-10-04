use std::str;
use std::str::FromStr;

use rusqlite as sql;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

use crate::crypto::PublicKey;
use crate::identity::Id;

impl FromSql for Id {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(id) => {
                let id = str::from_utf8(id).map_err(|e| FromSqlError::Other(Box::new(e)))?;
                let id = Id::from_str(id).map_err(|e| FromSqlError::Other(Box::new(e)))?;

                Ok(id)
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl ToSql for Id {
    fn to_sql(&self) -> sql::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_string()))
    }
}

impl FromSql for PublicKey {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(pk) => {
                let pk = str::from_utf8(pk).map_err(|e| FromSqlError::Other(Box::new(e)))?;
                let pk = PublicKey::from_str(pk).map_err(|e| FromSqlError::Other(Box::new(e)))?;

                Ok(pk)
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl ToSql for PublicKey {
    fn to_sql(&self) -> sql::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_string()))
    }
}
