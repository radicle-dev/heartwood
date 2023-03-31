// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use git_ext::Oid;
use nonempty::NonEmpty;
use radicle_crypto::PublicKey;
use serde::{Deserialize, Serialize};

use crate::object;
use crate::pruning_fold;

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
    /// The child entries for this entry.
    pub(super) children: Vec<EntryId>,
    /// The contents of this entry.
    pub(super) contents: Contents,
    /// The entry timestamp, as seconds since epoch.
    pub(super) timestamp: Timestamp,
}

impl Entry {
    pub fn new<Id1, Id2, ChildIds>(
        id: Id1,
        actor: PublicKey,
        resource: Oid,
        children: ChildIds,
        contents: Contents,
        timestamp: Timestamp,
    ) -> Self
    where
        Id1: Into<EntryId>,
        Id2: Into<EntryId>,
        ChildIds: IntoIterator<Item = Id2>,
    {
        Self {
            id: id.into(),
            actor,
            resource,
            children: children.into_iter().map(|id| id.into()).collect(),
            contents,
            timestamp,
        }
    }

    /// The ids of the changes this change depends on
    pub fn children(&self) -> impl Iterator<Item = &EntryId> {
        self.children.iter()
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

    pub fn id(&self) -> &EntryId {
        &self.id
    }
}

impl pruning_fold::GraphNode for Entry {
    type Id = EntryId;

    fn id(&self) -> &Self::Id {
        &self.id
    }

    fn child_ids(&self) -> &[Self::Id] {
        &self.children
    }
}

/// Wraps an [`Entry`], adding a logical clock to it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntryWithClock {
    pub entry: Entry,
    pub clock: Clock,
}

impl EntryWithClock {
    pub fn root(entry: Entry) -> Self {
        Self {
            entry,
            clock: 1 as Clock, // The root entry has a clock value of `1`.
        }
    }
}

impl EntryWithClock {
    /// Get the clock value.
    pub fn clock(&self) -> Clock {
        self.clock
    }

    /// Iterator over the changes, including the clock.
    pub fn changes(&self) -> impl Iterator<Item = &[u8]> {
        self.contents.iter().map(|blob| blob.as_slice())
    }
}

impl pruning_fold::GraphNode for EntryWithClock {
    type Id = EntryId;

    fn id(&self) -> &Self::Id {
        &self.entry.id
    }

    fn child_ids(&self) -> &[Self::Id] {
        &self.entry.children
    }
}

impl std::ops::Deref for EntryWithClock {
    type Target = Entry;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}
