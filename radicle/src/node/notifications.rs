pub mod store;

use localtime::LocalTime;
use sqlite as sql;
use thiserror::Error;

use crate::cob::object::ParseObjectId;
use crate::cob::{ObjectId, TypeName, TypeNameParse};
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

#[derive(Debug, PartialEq, Eq, Clone)]
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
#[derive(Debug, PartialEq, Eq, Clone)]
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
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum NotificationKind {
    /// A COB changed.
    Cob { type_name: TypeName, id: ObjectId },
    /// A source branch changed.
    Branch { name: BranchName },
}

#[derive(Error, Debug)]
pub enum NotificationKindError {
    /// Invalid COB type name.
    #[error("invalid type name: {0}")]
    TypeName(#[from] TypeNameParse),
    /// Invalid COB object id.
    #[error("invalid object id: {0}")]
    ObjectId(#[from] ParseObjectId),
    /// Invalid Git ref format.
    #[error("invalid ref format: {0}")]
    RefFormat(#[from] radicle_git_ext::ref_format::Error),
    /// Unknown notification kind.
    #[error("unknown notification kind {0:?}")]
    Unknown(Qualified<'static>),
}

impl<'a> TryFrom<Qualified<'a>> for NotificationKind {
    type Error = NotificationKindError;

    fn try_from(value: Qualified) -> Result<Self, Self::Error> {
        let kind = match value.non_empty_iter() {
            ("refs", "heads", head, rest) => NotificationKind::Branch {
                name: [head]
                    .into_iter()
                    .chain(rest)
                    .collect::<Vec<_>>()
                    .join("/")
                    .try_into()?,
            },
            ("refs", "cobs", type_name, id) => NotificationKind::Cob {
                type_name: type_name.parse()?,
                id: id.collect::<String>().parse()?,
            },
            _ => {
                return Err(NotificationKindError::Unknown(value.to_owned()));
            }
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
