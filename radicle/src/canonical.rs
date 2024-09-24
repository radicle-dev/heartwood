
//! A [Canonical JSON] formatter that escapes control characters. This
//! differs to the olpc-cjson standard.
//!
//! The [`olpc-cjson`] crate itself states:
//!
//! > OLPC’s canonical JSON specification is subtly different from
//! > other “canonical JSON” specifications, and is also not a strict
//! > subset of JSON (specifically, ASCII control characters 0x00–0x1f
//! > are printed literally, which is not valid JSON). Therefore,
//! > serde_json cannot necessarily deserialize JSON produced by this
//! > formatter.
//!
//! [Canonical JSON]: http://wiki.laptop.org/go/Canonical_JSON
//! [olpc-json]: https://docs.rs/olpc-cjson/0.1.2/olpc_cjson

pub mod formatter;
