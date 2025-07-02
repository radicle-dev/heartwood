pub mod bounded;
pub mod deserializer;
pub mod service;
pub mod wire;
pub mod worker;

pub mod connections;
pub mod tasks;

/// Peer-to-peer protocol version.
pub const PROTOCOL_VERSION: u8 = 1;
