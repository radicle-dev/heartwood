use std::ops::{Deref, DerefMut};

use automerge::transaction::Transactable;
use automerge::AutomergeError;

use crate::cob::automerge::value::Value;

/// Wraps an automerge transaction with additional functionality.
#[derive(Debug)]
pub struct Transaction<'a, 'b> {
    raw: &'a mut automerge::transaction::Transaction<'b>,
}

impl<'a, 'b> AsMut<automerge::transaction::Transaction<'b>> for Transaction<'a, 'b> {
    fn as_mut(&mut self) -> &mut automerge::transaction::Transaction<'b> {
        self.raw
    }
}

impl<'a, 'b> Deref for Transaction<'a, 'b> {
    type Target = automerge::transaction::Transaction<'b>;

    fn deref(&self) -> &Self::Target {
        self.raw
    }
}

impl<'a, 'b> DerefMut for Transaction<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.raw
    }
}

impl<'a, 'b> Transaction<'a, 'b> {
    pub fn new(raw: &'a mut automerge::transaction::Transaction<'b>) -> Self {
        Self { raw }
    }

    pub fn try_get<O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> Result<Option<(Value, automerge::ObjId)>, TransactionError> {
        let prop = prop.into();
        let result = self.raw.get(obj, prop)?;

        Ok(result)
    }

    pub fn get<O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> Result<(Value, automerge::ObjId), TransactionError> {
        let prop = prop.into();

        self.raw
            .get(obj, prop.clone())?
            .ok_or(TransactionError::PropertyNotFound(prop))
    }
}

/// Transaction error.
#[derive(thiserror::Error, Debug)]
pub enum TransactionError {
    #[error(transparent)]
    Automerge(#[from] AutomergeError),
    #[error("property '{0}' was not found in object")]
    PropertyNotFound(automerge::Prop),
    #[error("invalid property value for '{0}'")]
    InvalidValue(&'static str),
}
