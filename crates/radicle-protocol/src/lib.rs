pub mod bounded;
pub mod connections;
pub mod deserializer;
pub mod fetcher;
pub mod service;
pub mod wire;
pub mod worker;

/// Peer-to-peer protocol version.
pub const PROTOCOL_VERSION: u8 = 1;
