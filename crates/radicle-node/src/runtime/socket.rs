//! Type-safe socket path handling with platform-specific validation.
//!
//! This module provides newtype wrappers around [`PathBuf`] that enforce
//! platform-specific constraints for Unix domain sockets and Windows named
//! pipes.
//!
//! # Platform Constraints
//!
//! ## Unix Domain Sockets (size of `sockaddr_un.sun_path`)
//!
//! The maximum length of the socket path, usually related to the size of
//! `sockaddr_un.sun_path`, differs between operating systems.
//!
//! The following lengths (in C `char`s, assumed to match bytes) are known:
//! - 104: [macOS] (and thus iOS), [FreeBSD] (and thus DragonFly),
//!   [NetBSD], [OpenBSD]
//! - 108: [Linux] (and thus Android)
//!
//! On other Unix-like operating systems, a conservative length
//! of 104 bytes is used.
//!
//! ## Windows Named Pipes
//!
//! Windows imposes the following constraints on paths for named pipes,
//! see [`CreateNamedPipeW`]:
//! - Length up to 256 characters.
//! - Format: `\\.\pipe\<pipename>` or `\\<host>\pipe\<pipename>`
//! - `<pipename>` must exclude backslashes.
//!
//! [Linux]: https://man7.org/linux/man-pages/man7/unix.7.html
//! [macOS]: https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/man/man4/unix.4
//! [FreeBSD]: https://man.freebsd.org/cgi/man.cgi?unix(4)
//! [NetBSD]: https://man.netbsd.org/unix.4
//! [OpenBSD]: https://man.openbsd.org/unix.4
//! [`CreateNamedPipeW`]: https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew#parameters

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub mod error;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::net::UnixListener as Listener;
#[cfg(windows)]
use winpipe::WinListener as Listener;

/// Wraps a [`Listener`] but tracks its origin.
pub enum ControlSocket {
    /// The listener was created by binding to it.
    Bound(Listener, SocketPath),
    /// The listener was received via socket activation.
    Received(Listener),
}

impl ControlSocket {
    pub fn bind(path: SocketPath) -> Result<Self, error::ControlSocket> {
        #[cfg(all(feature = "systemd", target_os = "linux"))]
        {
            if let Some(listener) = Self::receive_listener() {
                log::info!(target: "node", "Received control socket.");
                return Ok(ControlSocket::Received(listener));
            }
        }

        // log::info!(target: "node", "Binding control socket {}..", &path.display());
        match Listener::bind(&path) {
            Ok(sock) => Ok(ControlSocket::Bound(sock, path)),
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
                Err(error::ControlSocket::AlreadyRunning {
                    path: path.into_path_buf(),
                })
            }
            Err(err) => Err(error::ControlSocket::Bind {
                path: path.into_path_buf(),
                source: err,
            }),
        }
    }

    #[cfg(all(feature = "systemd", target_os = "linux"))]
    fn receive_listener() -> Option<Listener> {
        let fd = match radicle_systemd::listen::fd("control") {
            Ok(Some(fd)) => fd,
            Ok(None) => return None,
            Err(err) => {
                log::error!(target: "node", "Error receiving listener from systemd: {err}");
                return None;
            }
        };

        let socket: socket2::Socket = unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) };

        let domain = match socket.domain() {
            Ok(domain) => domain,
            Err(err) => {
                log::error!(target: "node", "Error receiving listener from systemd when inspecting domain of socket: {err}");
                return None;
            }
        };

        if domain != socket2::Domain::UNIX {
            log::error!(target: "node", "Dropping listener received from systemd: Domain is not AF_UNIX.");
            return None;
        }

        Some(Listener::from(socket))
    }
}

/// A cross-platform socket path that works on both Unix and Windows.
///
/// On Unix systems, this wraps a [`UnixSocketPath`].
/// On Windows, this wraps a [`WindowsPipePath`].
///
/// This type provides a unified API for socket paths across platforms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SocketPath {
    #[cfg(unix)]
    Unix(unix::UnixSocketPath),
    #[cfg(windows)]
    Windows(windows::WindowsPipePath),
}

impl SocketPath {
    /// Creates a new socket path appropriate for the current platform.
    ///
    /// On Unix, this creates a `UnixSocketPath`.
    /// On Windows, this creates a `WindowsPipePath`.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, error::SocketPath> {
        #[cfg(unix)]
        {
            unix::UnixSocketPath::new(path).map(SocketPath::Unix)
        }
        #[cfg(windows)]
        {
            windows::WindowsPipePath::new(path).map(SocketPath::Windows)
        }
    }

    /// Returns the path as a `Path` reference.
    pub fn as_path(&self) -> &Path {
        match self {
            #[cfg(unix)]
            SocketPath::Unix(p) => p.as_path(),
            #[cfg(windows)]
            SocketPath::Windows(p) => p.as_path(),
        }
    }

    /// Consumes the socket path and returns the inner `PathBuf`.
    pub fn into_path_buf(self) -> PathBuf {
        match self {
            #[cfg(unix)]
            SocketPath::Unix(p) => p.into_path_buf(),
            #[cfg(windows)]
            SocketPath::Windows(p) => p.into_path_buf(),
        }
    }
}

impl AsRef<Path> for SocketPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl fmt::Display for SocketPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(unix)]
            SocketPath::Unix(p) => write!(f, "{}", p),
            #[cfg(windows)]
            SocketPath::Windows(p) => write!(f, "{}", p),
        }
    }
}
