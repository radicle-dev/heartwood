use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ControlSocket {
    /// Another node is already running.
    #[error(
        "another node appears to be running; \
        if this isn't the case, delete the socket file at '{path}' \
        and restart the node"
    )]
    AlreadyRunning { path: PathBuf },
    #[error("failed to bind listener to '{path}': {source}")]
    Bind { path: PathBuf, source: io::Error },
}

/// Errors that can occur during socket path validation.
#[derive(Debug, Error)]
pub enum SocketPath {
    /// The path exceeds the maximum allowed length for the platform.
    #[error("socket path `{path}` too long: {length} bytes exceeds maximum of {max} bytes")]
    PathTooLong {
        path: String,
        length: usize,
        max: usize,
    },
    /// The path contains a null byte, which is not allowed.
    #[error("socket path contains null byte at position {position}")]
    ContainsNullByte { position: usize },
    /// The path is empty.
    #[error("socket path cannot be empty")]
    EmptyPath,

    /// The path contains invalid UTF-8.
    #[cfg(windows)]
    #[error("socket path '{path}' contains invalid UTF-8")]
    InvalidUtf8 { path: PathBuf },
    /// The path does not follow the required Windows named pipe format.
    #[cfg(windows)]
    #[error("invalid Windows pipe format '{pipe_name}': {reason}")]
    InvalidPipeFormat { reason: &'static str },
    /// The pipe name portion contains a backslash.
    #[cfg(windows)]
    #[error("pipe name '{pipe_name}' cannot contain backslashes")]
    PipeNameContainsBackslash { pipe_name: String },
}
