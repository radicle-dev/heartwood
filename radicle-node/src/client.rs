use std::io;

use thiserror::Error;

use crate::address;
use crate::service::{routing, tracking};

pub mod handle;

/// Directory in `$RAD_HOME` under which node-specific files are stored.
pub const NODE_DIR: &str = "node";
/// Filename of routing table database under [`NODE_DIR`].
pub const ROUTING_DB_FILE: &str = "routing.db";
/// Filename of address database under [`NODE_DIR`].
pub const ADDRESS_DB_FILE: &str = "addresses.db";
/// Filename of tracking table database under [`NODE_DIR`].
pub const TRACKING_DB_FILE: &str = "tracking.db";

/// A client error.
#[derive(Error, Debug)]
pub enum Error {
    /// A routing database error.
    #[error("routing database error: {0}")]
    Routing(#[from] routing::Error),
    /// An address database error.
    #[error("address database error: {0}")]
    Addresses(#[from] address::Error),
    /// A tracking database error.
    #[error("tracking database error: {0}")]
    Tracking(#[from] tracking::Error),
    /// An I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// A networking error.
    #[error("network error: {0}")]
    Net(#[from] nakamoto_net::error::Error),
}
