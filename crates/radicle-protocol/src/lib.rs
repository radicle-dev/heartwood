pub mod bounded;
pub mod deserializer;
pub mod service;
pub mod wire;
pub mod worker;

/// Peer-to-peer protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

extern crate radicle_localtime as localtime;
