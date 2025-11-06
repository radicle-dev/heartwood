#![no_std]

//! This a crate for defining core data type for the Radicle protocol, such as
//! [`RepoId`].
//!
//! # Feature Flags
//!
//! The only default feature is `std`.
//!
//! ## `std`
//!
//! [`OsString`]: ::doc_std::ffi::OsString
//!
//! Provides implementation of [`TryFrom<OsString>`].
//!
//! Enabled by default, since it is expected that most dependents will use the
//! standard library.
//!
//! ## `git2`
//!
//! [`git2::Oid`]: ::git2::Oid
//!
//! Provides conversion from a [`git2::Oid`] to a [`RepoId`].
//!
//! ## `gix`
//!
//! [`ObjectId`]: ::gix_hash::ObjectId
//!
//! Provides conversion from a [`ObjectId`] to a [`RepoId`].
//!
//! ## `radicle-git-ref-format`
//!
//! Provides conversions from data types defined in `radicle-core` into valid
//! reference components and/or strings.
//!
//! ## `serde`
//!
//! [`Serialize`]: ::serde::ser::Serialize
//! [`Deserialize`]: ::serde::de::Deserialize
//!
//! Provides implementations of [`Serialize`] and [`Deserialize`].
//!
//! ## `schemars`
//!
//! [`JsonSchema`]: ::schemars::JsonSchema
//!
//! Provides implementations of [`JsonSchema`].
//!
//! ## `proptest`
//!
//! [`proptest::Strategy`]: ::proptest::strategy::Strategy
//!
//! Provides functions for generating different types of [`proptest::Strategy`].
//!
//! ## `qcheck`
//!
//! [`qcheck::Arbitrary`]: ::qcheck::Arbitrary
//!
//! Provides implementations of [`qcheck::Arbitrary`].
//!
//! ## `sqlite`
//!
//! [`sqlite::BindableWithIndex`]: ::sqlite::BindableWithIndex
//! [`sqlite::Value`]: ::sqlite::Value
//!
//! Provides implementations of [`sqlite::BindableWithIndex`] and `TryFrom`
//! implementations from the [`sqlite::Value`] type to the domain type.

#[cfg(doc)]
extern crate std as doc_std;

extern crate alloc;

pub mod repo;
pub use repo::RepoId;
