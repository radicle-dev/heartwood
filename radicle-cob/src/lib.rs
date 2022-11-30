// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

//! # Collaborative Objects
//!
//! Collaborative objects are graphs of CRDTs. The current CRDTs that
//! is intended to be used are specifically [automerge] CRDTs.
//!
//! The initial design is proposed at [RFC-0662], and this
//! implementation keeps to most of its design principle.
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
//! This `Resource` must implement [`identity::Identity`] to allow the
//! internal logic to reference the resource's content-address in
//! `git` as well as the stable identifier used for the resource.
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
//! point that the `author`'s change can be filtered out from the
//! final product of the traversal.
//!
//! [automerge]: https://automerge.org
//! [RFC-0662]: https://github.com/radicle-dev/radicle-link/blob/master/docs/rfc/0662-collaborative-objects.adoc

#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

extern crate radicle_crypto as crypto;
extern crate radicle_git_ext as git_ext;

mod backend;
pub use backend::git;

mod change_graph;
mod trailers;

pub mod change;
pub use change::Change;

pub mod identity;

pub mod history;
pub use history::{Contents, Entry, History};

mod pruning_fold;

pub mod signatures;
use signatures::Signature;

pub mod type_name;
pub use type_name::TypeName;

pub mod object;
pub use object::{create, get, info, list, update, CollaborativeObject, Create, ObjectId, Update};

#[cfg(test)]
mod test;

#[cfg(test)]
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
pub trait Store
where
    Self: object::Storage
        + change::Storage<
            CreateError = git::change::error::Create,
            LoadError = git::change::error::Load,
            ObjectId = git_ext::Oid,
            Author = git_ext::Oid,
            Resource = git_ext::Oid,
            Signatures = Signature,
        >,
{
}
