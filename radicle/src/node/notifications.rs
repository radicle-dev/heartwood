pub mod store;

use localtime::LocalTime;
use serde::Serialize;
use sqlite as sql;
use thiserror::Error;

use crate::cob;
use crate::cob::TypedId;
use crate::git::{BranchName, Qualified};
use crate::prelude::RepoId;
use crate::storage::{RefUpdate, RemoteId};

pub use store::{Error, Store};
/// Read and write to the store.
pub type StoreWriter = Store<store::Write>;
/// Write to the store.
pub type StoreReader = Store<store::Read>;

/// Unique identifier for a notification.
pub type NotificationId = u32;

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum NotificationStatus {
    ReadAt(LocalTime),
    Unread,
}

impl NotificationStatus {
    pub fn is_read(&self) -> bool {
        matches!(self, Self::ReadAt(_))
    }
}

/// A notification for an updated ref.
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub struct Notification {
    /// Unique notification ID.
    pub id: NotificationId,
    /// Source repository for this notification.
    pub repo: RepoId,
    /// Remote, if any.
    pub remote: Option<RemoteId>,
    /// Qualified ref name that was updated.
    pub qualified: Qualified<'static>,
    /// The underlying ref update.
    pub update: RefUpdate,
    /// Notification kind.
    pub kind: NotificationKind,
    /// Read status.
    pub status: NotificationStatus,
    /// Timestamp of the update.
    pub timestamp: LocalTime,
}

/// Type of notification.
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum NotificationKind {
    /// A COB changed.
    Cob {
        #[serde(flatten)]
        typed_id: TypedId,
    },
    /// A source branch changed.
    Branch { name: BranchName },
    /// Unknown reference.
    Unknown { refname: Qualified<'static> },
}

#[derive(Error, Debug)]
pub enum NotificationKindError {
    #[error("invalid cob identifier: {0}")]
    TypedId(#[from] cob::ParseIdentifierError),
    /// Invalid Git ref format.
    #[error("invalid ref format: {0}")]
    RefFormat(#[from] radicle_git_ext::ref_format::Error),
}

impl TryFrom<Qualified<'_>> for NotificationKind {
    type Error = NotificationKindError;

    fn try_from(value: Qualified) -> Result<Self, Self::Error> {
        let kind = match TypedId::from_qualified(&value)? {
            Some(typed_id) => Self::Cob { typed_id },
            None => match value.non_empty_iter() {
                ("refs", "heads", head, rest) => Self::Branch {
                    name: [head]
                        .into_iter()
                        .chain(rest)
                        .collect::<Vec<_>>()
                        .join("/")
                        .try_into()?,
                },
                _ => Self::Unknown {
                    refname: value.to_owned(),
                },
            },
        };
        Ok(kind)
    }
}

impl TryFrom<&sql::Value> for NotificationStatus {
    type Error = sql::Error;

    fn try_from(value: &sql::Value) -> Result<Self, Self::Error> {
        match value {
            sql::Value::Null => Ok(NotificationStatus::Unread),
            sql::Value::Integer(i) => Ok(NotificationStatus::ReadAt(LocalTime::from_millis(
                *i as u128,
            ))),
            _ => Err(sql::Error {
                code: None,
                message: Some("sql: invalid type for notification status".to_owned()),
            }),
        }
    }
}

impl sql::BindableWithIndex for &NotificationStatus {
    fn bind<I: sql::ParameterIndex>(self, stmt: &mut sql::Statement<'_>, i: I) -> sql::Result<()> {
        match self {
            NotificationStatus::Unread => sql::Value::Null.bind(stmt, i),
            NotificationStatus::ReadAt(t) => {
                sql::Value::Integer(t.as_millis() as i64).bind(stmt, i)
            }
        }
    }
}
