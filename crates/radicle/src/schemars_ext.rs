//! This module contains auxiliary definitions for generating JSONSchemas.
//! See <https://graham.cool/schemars/examples/5-remote_derive/>.
#![allow(dead_code)]

use schemars::JsonSchema;

pub mod crypto {
    use super::*;
    /// See [`crate::node::NodeId`]
    /// See [`crate::storage::RemoteId`]
    /// See [`::crypto::PublicKey`]
    ///
    /// An Ed25519 public key in multibase encoding.
    ///
    /// `MULTIBASE(base58-btc, MULTICODEC(public-key-type, raw-public-key-bytes))`
    #[derive(JsonSchema)]
    #[schemars(
    title = "NodeId",
    description = "An Ed25519 public key in multibase encoding.",
    extend("examples" = [
        "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7",
        "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C",
        "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
        "z6MkkfM3tPXNPrPevKr3uSiQtHPuwnNhu2yUVjgd2jXVsVz5",
    ]),
)]
    pub struct PublicKey(String);
}

pub(crate) mod log {
    use super::*;

    /// See [`::log::Level`]
    #[derive(JsonSchema)]
    #[schemars(
        remote = "log::Level",
        description = "A log level.",
        rename_all = "UPPERCASE"
    )]
    pub(crate) enum Level {
        /// Designates very serious errors.
        Error,
        /// Designates hazardous situations.
        Warn,
        /// Designates useful information.
        Info,
        /// Designates lower priority information.
        Debug,
        /// Designates very low priority, often extremely verbose, information.
        Trace,
    }
}

pub(crate) mod bytesize {
    use super::*;

    /// See [`::bytesize::ByteSize`] as well as [`::bytesize::parse`].
    /// Note that the pattern here is a little more restrictive than
    /// the actual parsing logic, as it enforces particular casing and whitespace.
    /// However, the regular expression is easier to read.
    #[derive(JsonSchema)]
    #[schemars(
        remote = "bytesize::ByteSize",
        description = "Byte quantities using unit prefixes according to SI or ISO/IEC 80000-13.",
        extend("examples" = ["7 G", "50.3 TiB", "200 B", "4 Ki", "10 MB"]),
    )]
    pub(crate) struct ByteSize(
        #[schemars(regex(pattern = r"^\d+(\.\d+)? ((K|M|G|T|P)i?B?|B)$"))] String,
    );
}

pub(crate) mod localtime {
    use super::*;

    /// See [`::localtime::LocalDuration`]
    #[derive(JsonSchema)]
    #[schemars(
        remote = "localtime::LocalDuration",
        description = "A time duration measured locally in milliseconds."
    )]
    pub(crate) struct LocalDuration(u64);

    /// See [`crate::serde_ext::localtime::time`]
    #[derive(JsonSchema)]
    #[schemars(
        remote = "localtime::LocalDuration",
        description = "A time duration measured locally in seconds."
    )]
    pub(crate) struct LocalDurationInSeconds(u64);
}

pub(crate) mod git {
    use super::*;

    /// See [`crate::git::Oid`]
    /// See [`::git_ext::Oid`]
    /// See [`::git2::Oid`]
    ///
    /// A Git Object Identifier in hexadecimal encoding.
    #[derive(JsonSchema)]
    #[schemars(
        remote = "git2::Oid",
        description = "A Git Object Identifier (SHA-1 or SHA-256 hash) in hexadecimal encoding."
    )]
    pub(crate) struct Oid(
        #[schemars(regex(pattern = r"^([0-9a-fA-F]{64}|[0-9a-fA-F]{40})$"))] String,
    );

    /// See [`crate::git::RefString`]
    #[derive(JsonSchema)]
    pub(crate) struct RefString(String);
}
