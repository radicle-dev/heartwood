use std::io;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::*;

/// Operating system signal.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Signal {
    /// `SIGINT`.
    Interrupt,
    /// `SIGTERM`.
    Terminate,
    /// `SIGHUP`.
    Hangup,
    /// `SIGWINCH`.
    WindowChanged,
}

/// Return an error indicating that signal handling is already installed.
#[inline(always)]
fn already_installed() -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        "signal handling is already installed",
    )
}
