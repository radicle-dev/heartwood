// N.b. Rust 1.85 introduced some annoying clippy warnings about using `b""`
// syntax in place of `b''`, but in our cases they were u8 and not [u8] so the
// suggestions did not make sense.
#![allow(clippy::byte_char_slices)]

pub mod fingerprint;
pub(crate) mod reactor;
pub mod runtime;

mod control;
mod wire;
mod worker;

#[cfg(any(test, feature = "test"))]
pub mod test;
#[cfg(test)]
pub mod tests;

extern crate radicle_fetch as fetch;
extern crate radicle_localtime as localtime;
extern crate radicle_protocol as protocol;

use radicle::version::Version;

/// Node version.
pub const VERSION: Version = Version {
    name: env!("CARGO_PKG_NAME"),
    commit: env!("GIT_HEAD"),
    version: env!("RADICLE_VERSION"),
    timestamp: env!("SOURCE_DATE_EPOCH"),
};
