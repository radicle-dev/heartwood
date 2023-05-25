// Copyright © 2021 The Radicle Link Contributors

use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use git_ext::Oid;
use nonempty::NonEmpty;
use radicle_crypto::PublicKey;
use serde::{Deserialize, Serialize};

use crate::{object, ObjectId};

/// Entry contents.
/// This is the change payload.
pub type Contents = NonEmpty<Vec<u8>>;

/// Logical clock used to track causality in change graph.
pub type Clock = u64;

/// Local time in seconds since epoch.
pub type Timestamp = u64;

/// A unique identifier for a history entry.
#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntryId(Oid);

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for EntryId {
    type Err = git_ext::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let oid = git_ext::Oid::try_from(s)?;

        Ok(Self(oid))
    }
}

impl From<git2::Oid> for EntryId {
    fn from(id: git2::Oid) -> Self {
        Self(id.into())
    }
}

impl From<Oid> for EntryId {
    fn from(id: Oid) -> Self {
        Self(id)
    }
}

impl From<EntryId> for Oid {
    fn from(EntryId(id): EntryId) -> Self {
        id
    }
}

impl From<&EntryId> for object::ObjectId {
    fn from(EntryId(id): &EntryId) -> Self {
        id.into()
    }
}

impl From<ObjectId> for EntryId {
    fn from(id: ObjectId) -> Self {
        Self(*id)
    }
}

impl Deref for EntryId {
    type Target = Oid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// One entry in the dependency graph for a change
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entry {
    /// The identifier for this entry
    pub(super) id: EntryId,
    /// The actor that authored this entry.
    pub(super) actor: PublicKey,
    /// The content-address for the resource this entry lives under.
    /// If the resource was updated, this should point to its latest version.
    pub(super) resource: Oid,
    /// The contents of this entry.
    pub(super) contents: Contents,
    /// The entry timestamp, as seconds since epoch.
    pub(super) timestamp: Timestamp,
    /// Logical clock.
    pub(super) clock: Clock,
}

impl Entry {
    pub fn new<Id>(
        id: Id,
        actor: PublicKey,
        resource: Oid,
        contents: Contents,
        timestamp: Timestamp,
        clock: Clock,
    ) -> Self
    where
        Id: Into<EntryId>,
    {
        Self {
            id: id.into(),
            actor,
            resource,
            contents,
            timestamp,
            clock,
        }
    }

    /// The current `Oid` of the resource this change lives under.
    pub fn resource(&self) -> Oid {
        self.resource
    }

    /// The public key of the actor.
    pub fn actor(&self) -> &PublicKey {
        &self.actor
    }

    /// The entry timestamp.
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// The contents of this change
    pub fn contents(&self) -> &Contents {
        &self.contents
    }

    /// Entry ID.
    pub fn id(&self) -> &EntryId {
        &self.id
    }

    /// Logical clock.
    pub fn clock(&self) -> Clock {
        self.clock
    }
}
