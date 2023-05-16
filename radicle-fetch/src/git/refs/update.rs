//! The set of types that describe performing updates to a Git
//! repository.
//!
//! An [`Update`] describes a single update that can made to a Git
//! repository. **Note** that it currently does not support symbolic
//! references.
//!
//! A group of `Update`s is described by [`Updates`] which groups
//! those updates by each peer's namespace, i.e. their [`PublicKey`].
//!
//! When an `Update` is successful the corresponding [`Updated`] is
//! expected to be produced.
//!
//! The final result of applying a set of [`Updates`] is captured in
//! the [`Applied`] type, which contains any rejected, but non-fatal,
//! [`Update`]s and successful [`Updated`] values.

use std::collections::BTreeMap;

use either::Either;
use radicle::git::{Namespaced, Oid, Qualified};
use radicle::prelude::PublicKey;

pub use radicle::storage::RefUpdate;

/// The set of applied changes from a reference store update.
#[derive(Debug, Default)]
pub struct Applied<'a> {
    /// Set of rejected updates if they did not meet the update
    /// requirements, e.g. concurrent change to previous object id,
    /// broke fast-forward policy, etc.
    pub rejected: Vec<Update<'a>>,
    /// Set of successfully updated references.
    pub updated: Vec<RefUpdate>,
}

impl<'a> Applied<'a> {
    pub fn append(&mut self, other: &mut Self) {
        self.rejected.append(&mut other.rejected);
        self.updated.append(&mut other.updated);
    }
}

/// A set of [`Update`]s that are grouped by which namespace they are
/// affecting.
#[derive(Clone, Default, Debug)]
pub struct Updates<'a> {
    pub tips: BTreeMap<PublicKey, Vec<Update<'a>>>,
}

impl<'a> Updates<'a> {
    pub fn build(updates: impl IntoIterator<Item = (PublicKey, Update<'a>)>) -> Self {
        let tips = updates.into_iter().fold(
            BTreeMap::<_, Vec<Update<'a>>>::new(),
            |mut tips, (remote, up)| {
                tips.entry(remote)
                    .and_modify(|ups| ups.push(up.clone()))
                    .or_insert(vec![up]);
                tips
            },
        );
        Self { tips }
    }

    pub fn add(&mut self, remote: PublicKey, up: Update<'a>) {
        self.tips
            .entry(remote)
            .and_modify(|ups| ups.push(up.clone()))
            .or_insert(vec![up]);
    }

    pub fn append(&mut self, remote: PublicKey, mut new: Vec<Update<'a>>) {
        self.tips
            .entry(remote)
            .and_modify(|ups| ups.append(&mut new))
            .or_insert(new);
    }
}

/// The policy to follow when an [`Update::Direct`] is not a
/// fast-forward.
#[derive(Clone, Copy, Debug)]
pub enum Policy {
    /// Abort the entire transaction.
    Abort,
    /// Reject this update, but continue the transaction.
    Reject,
    /// Allow the update.
    Allow,
}

/// An update that can be applied to a Git repository.
#[derive(Clone, Debug)]
pub enum Update<'a> {
    /// Update a direct reference, i.e. a reference that points to an
    /// object.
    Direct {
        /// The name of the reference that is being updated.
        name: Namespaced<'a>,
        /// The resulting target of the reference that is being
        /// updated.
        target: Oid,
        /// Policy to apply when an [`Update`] would not apply as a
        /// fast-forward.
        no_ff: Policy,
    },
    /// Delete a reference.
    Prune {
        /// The name of the reference that is being deleted.
        name: Namespaced<'a>,
        /// The previous value of the reference.
        ///
        /// It can either be a direct reference pointing to an
        /// [`Oid`], or a symbolic reference pointing to a
        /// [`Qualified`] reference name.
        prev: Either<Oid, Qualified<'a>>,
    },
}

impl<'a> Update<'a> {
    pub fn refname(&self) -> &Namespaced<'a> {
        match self {
            Update::Direct { name, .. } => name,
            Update::Prune { name, .. } => name,
        }
    }

    pub fn into_owned<'b>(self) -> Update<'b> {
        match self {
            Self::Direct {
                name,
                target,
                no_ff,
            } => Update::Direct {
                name: name.into_owned(),
                target,
                no_ff,
            },
            Self::Prune { name, prev } => Update::Prune {
                name: name.into_owned(),
                prev: prev.map_right(|q| q.into_owned()),
            },
        }
    }
}
