use std::{
    fmt,
    num::TryFromIntError,
    ops::{Add, Deref, Sub},
};

use localtime::LocalTime;
use sqlite as sql;

/// Milliseconds since epoch.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Add<u64> for Timestamp {
    type Output = Timestamp;

    fn add(self, millis: u64) -> Self::Output {
        Self(self.0.saturating_add(millis))
    }
}

impl Sub<u64> for Timestamp {
    type Output = Timestamp;

    fn sub(self, millis: u64) -> Self::Output {
        Self(self.0.saturating_sub(millis))
    }
}

impl Timestamp {
    /// UNIX epoch.
    pub const EPOCH: Self = Self(0);
    /// Minimum value.
    pub const MIN: Self = Self(0);
    /// Maximum value.
    // Nb. This is the maximum value that can fit in a signed 64-bit integer (`i64`).
    // This makes it possible to store timestamps in sqlite.
    pub const MAX: Self = Self(9223372036854775807);

    /// Convert to local time.
    pub fn to_local_time(&self) -> LocalTime {
        (*self).into()
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Timestamp {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<LocalTime> for Timestamp {
    fn from(t: LocalTime) -> Self {
        Self(t.as_millis())
    }
}

impl From<Timestamp> for LocalTime {
    fn from(t: Timestamp) -> Self {
        LocalTime::from_millis(t.0 as u128)
    }
}

impl TryFrom<u64> for Timestamp {
    type Error = u64;

    fn try_from(u: u64) -> Result<Self, u64> {
        if u <= *Self::MAX {
            Ok(Self(u))
        } else {
            Err(u)
        }
    }
}

impl TryFrom<i64> for Timestamp {
    type Error = TryFromIntError;

    fn try_from(i: i64) -> Result<Self, Self::Error> {
        i.try_into().map(Self)
    }
}

impl TryFrom<&sql::Value> for Timestamp {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::Integer(i) => match (*i).try_into() {
                Ok(u) => Ok(Timestamp(u)),
                Err(e) => Err(sql::Error {
                    code: None,
                    message: Some(format!("sql: invalid integer `{i}` for timestamp: {e}")),
                }),
            },
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for timestamp".to_owned()),
            }),
        }
    }
}

impl sql::BindableWithIndex for &Timestamp {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        match i64::try_from(*self.deref()) {
            Ok(integer) => integer.bind(stmt, i),
            Err(e) => Err(sql::Error {
                code: None,
                message: Some(format!("sql: invalid timestamp `{self}`: {e}")),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_max() {
        assert_eq!(i64::try_from(*Timestamp::MAX), Ok(i64::MAX));
    }
}
