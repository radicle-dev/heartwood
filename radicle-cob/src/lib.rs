// Copyright © 2021 The Radicle Link Contributors

#![warn(clippy::unwrap_used)]
//! # Collaborative Objects
//!
//! Collaborative objects are graphs of CRDTs. The current CRDTs that
//! is intended to be used are specifically [automerge] CRDTs.
//!
//! ## Basic Types
//!
//! The basic types that are found in `radicle-cob` are:
//!   * [`CollaborativeObject`] -- the computed object itself.
//!   * [`ObjectId`] -- the content-address for a single collaborative
//!   object.
//!   * [`TypeName`] -- the name for a collection of collaborative objects.
//!   * [`History`] -- the traversable history of the changes made to
//!   a single collaborative object.
//!
//! ## CRU Interface (No Delete)
//!
//! The main entry for manipulating [`CollaborativeObject`]s is by
//! using the CRU like functions:
//!   * [`create`]
//!   * [`get`]
//!   * [`list`]
//!   * [`update`]
//!
//! ## Storage
//!
//! The storing of collaborative objects is based on a git
//! backend. The previously mentioned functions all accept a [`Store`]
//! as parameter. The `Store` itself is an accumulation of different
//! storage capabilities:
//!   * [`object::Storage`]
//!   * [`change::Storage`] -- **Note**: there is already an
//!   implementation for this for [`git2::Repository`] for convenience.
//!
//! ## Resource
//!
//! The [`create`] and [`update`] functions take a `Resource`. It
//! represents the type of resource the collaborative objects are
//! relating to, for example a software project.
//!
//! ## History Traversal
//!
//! The [`History`] of a [`CollaborativeObject`] -- accessed via
//! [`CollaborativeObject::history`] -- has a method
//! [`History::traverse`] which provides a way of inspecting each
//! [`Entry`] and building up a final value.
//!
//! This mechanism would be used in tandem with [automerge] to load an
//! automerge document and deserialize into an application defined
//! object.
//!
//! This traversal is also the point at which the [`Entry::author`]
//! and [`Entry::resource`] can be retrieved to apply any kind of
//! filtering logic. For example, a specific `author`'s change may be
//! egregious, spouting terrible libel about Radicle. It is at this
//! point that the `actor`'s change can be filtered out from the
//! final product of the traversal.
//!
//! [automerge]: https://automerge.org

#[cfg(test)]
extern crate qcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate qcheck_macros;

extern crate radicle_crypto as crypto;
extern crate radicle_git_ext as git_ext;

mod backend;
pub use backend::git;

mod change_graph;
mod trailers;

pub mod change;
pub use change::store::{Contents, Embed, EntryId, Manifest, Version};
pub use change::Entry;

pub mod history;
pub use history::History;

pub mod signatures;
use signatures::ExtendedSignature;

pub mod type_name;
pub use type_name::TypeName;

pub mod object;
pub use object::{
    create, get, info, list, remove, update, CollaborativeObject, Create, ObjectId, Update, Updated,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

/// The `Store` is an aggregation of the different types of storage
/// traits required for editing [`CollaborativeObject`]s.
///
/// The backing store being used is expected to be a `git` backend.
///
/// To get started using this trait, you must implement the following
/// for the specific `git` storage:
///
///   * [`object::Storage`]
///
/// **Note**: [`change::Storage`] is already implemented for
/// [`git2::Repository`]. It is expected that the underlying storage
/// for `object::Storage` will also be `git2::Repository`, but if not
/// please open an issue to change the definition of `Store` :)
pub trait Store<I = crypto::PublicKey>
where
    Self: object::Storage<Identifier = I>
        + change::Storage<
            StoreError = git::change::error::Create,
            LoadError = git::change::error::Load,
            ObjectId = git_ext::Oid,
            Parent = git_ext::Oid,
            Signatures = ExtendedSignature,
        >,
{
}
