//! Library for interaction with systemd, specialized for Radicle.

#[cfg(all(feature = "journal", target_os = "linux"))]
pub mod journal;

#[cfg(all(feature = "listen", target_os = "linux"))]
pub mod listen;
