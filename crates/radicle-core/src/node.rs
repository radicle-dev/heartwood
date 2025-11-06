//! A Radicle node on the network is identified by its [`NodeId`], which in turn
//! is a Ed25519 public key.
//!
//! The human-readable format is a multibase-encoded format of the underlying Ed25519 public key, i.e.
//! ```
//! MULTIBASE(base58-btc, MULTICODEC(public-key-type, raw-public-key-bytes))
//! ```
//! which results in strings that look like:
//! ```
//! z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
//! ```

use radicle_crypto::PublicKey;

/// Public identifier of a node device in the network.
///
/// # Legacy
///
/// This is a type alias, providing little protection around evolving a [`NodeId`]
/// and having it very tightly coupled with a [`PublicKey`].
///
/// Future iterations will change this to provide a better API for working with
/// [`NodeId`]'s and their usage in the protocol.
pub type NodeId = PublicKey;
