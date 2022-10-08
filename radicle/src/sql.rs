use std::str::FromStr;

use sqlite as sql;
use sqlite::Value;

use crate::crypto::PublicKey;
use crate::identity::Id;

impl sql::ValueInto for Id {
    fn into(value: &Value) -> Option<Self> {
        match value {
            Value::String(id) => Id::from_str(id).ok(),
            _ => None,
        }
    }
}

impl sqlite::Bindable for &Id {
    fn bind(self, stmt: &mut sql::Statement<'_>, i: usize) -> sql::Result<()> {
        self.to_human().as_str().bind(stmt, i)
    }
}

impl sql::ValueInto for PublicKey {
    fn into(value: &Value) -> Option<Self> {
        match value {
            Value::String(id) => PublicKey::from_str(id).ok(),
            _ => None,
        }
    }
}

impl sqlite::Bindable for &PublicKey {
    fn bind(self, stmt: &mut sql::Statement<'_>, i: usize) -> sql::Result<()> {
        self.to_human().as_str().bind(stmt, i)
    }
}
