#![deny(clippy::unwrap_used)]
//! Repository and actor identity.
//!
//! # Device vs person DIDs
//!
//! Radicle distinguishes **device** identities from **logical** (person/account)
//! identities:
//!
//! - [`Did::Key`] (`did:key:…`) is an Ed25519 device key. It is also the
//!   [`crate::prelude::NodeId`] used for Noise, git namespaces, and remotes.
//! - [`Did::Plc`] (`did:plc:…`) is an ATProto PLC account DID. It may act as a
//!   COB author, repository delegate, or follow target. Verifying keys are
//!   resolved with [`plc::DidResolver`]: pins in the `xyz.radicle.did` identity
//!   payload for authz-critical uses, and a local PLC cache for authorship.
//!
//! Signing remains Ed25519 only. A `did:plc` is usable when its DID document
//! exposes an Ed25519 Multikey (`#atproto` preferred, else `#radicle`).

pub mod crefs;
pub mod did;
pub mod doc;
pub mod plc;
pub mod project;

pub use crefs::CanonicalRefs;
pub use crypto::PublicKey;
pub use did::Did;
pub use doc::{Doc, DocAt, DocError, IdError, PayloadError, RawDoc, RepoId, Visibility};
pub use plc::{
    DidDocument, DidPayload, DidResolver, HybridResolver, KeyOnlyResolver, PinnedVerification,
    PlcCache, PlcId, ResolveError, ResolvedDid, key_acts_for,
};
pub use project::Project;

pub use crate::cob::identity::{Action, Error, Identity, IdentityMut, TYPENAME};
