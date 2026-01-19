use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};

use super::error;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub const MAX_PATH_LEN: usize = 108;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
pub const MAX_PATH_LEN: usize = 104;

// Fallback for other Unix-like systems, use conservative value
// that matches BSD family.
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    ))
))]
pub const MAX_PATH_LEN: usize = 104;

/// A validated Unix domain socket path.
///
/// This type guarantees that the contained path:
/// - Does have a length that fits the platform's `sun_path` size.
/// - Does not contain embedded null bytes.
/// - Is not empty.
///
/// # Example
///
/// ```ignore, rust
/// let path = UnixSocketPath::new("/tmp/my.sock")?;
/// let listener = std::os::unix::net::UnixListener::bind(path.as_path())?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnixSocketPath {
    inner: PathBuf,
}

impl UnixSocketPath {
    /// The maximum number of bytes allowed in a socket path (excluding null terminator).
    pub const MAX_PATH_BYTES: usize = MAX_PATH_LEN - 1;

    /// Creates a new `UnixSocketPath` after validating the path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is empty
    /// - The path exceeds `MAX_PATH_BYTES`
    /// - The path contains embedded null bytes
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, error::SocketPath> {
        let path = path.as_ref();
        Self::from_path_buf(path.to_path_buf())
    }

    /// Creates a new `UnixSocketPath` from a `PathBuf`.
    ///
    /// # Errors
    ///
    /// See [`UnixSocketPath::new`].
    pub fn from_path_buf(path: PathBuf) -> Result<Self, error::SocketPath> {
        use std::os::unix::ffi::OsStrExt;

        let bytes = path.as_os_str().as_bytes();

        // Check for empty path
        if bytes.is_empty() {
            return Err(error::SocketPath::EmptyPath);
        }

        // Check for null bytes
        if let Some(pos) = bytes.iter().position(|&b| b == 0) {
            return Err(error::SocketPath::ContainsNullByte { position: pos });
        }

        // Check length (must leave room for null terminator)
        if bytes.len() > Self::MAX_PATH_BYTES {
            return Err(error::SocketPath::PathTooLong {
                path: path.to_string_lossy().to_string(),
                length: bytes.len(),
                max: Self::MAX_PATH_BYTES,
            });
        }

        Ok(Self { inner: path })
    }

    /// Returns the path as a `Path` reference.
    #[inline]
    pub fn as_path(&self) -> &Path {
        &self.inner
    }

    /// Returns the path as an `OsStr` reference.
    #[inline]
    pub fn as_os_str(&self) -> &OsStr {
        self.inner.as_os_str()
    }

    /// Consumes the `UnixSocketPath` and returns the inner `PathBuf`.
    #[inline]
    pub fn into_path_buf(self) -> PathBuf {
        self.inner
    }
}

impl AsRef<Path> for UnixSocketPath {
    #[inline]
    fn as_ref(&self) -> &Path {
        &self.inner
    }
}

impl AsRef<OsStr> for UnixSocketPath {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        self.inner.as_os_str()
    }
}

impl std::ops::Deref for UnixSocketPath {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl fmt::Display for UnixSocketPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner.display())
    }
}

impl TryFrom<PathBuf> for UnixSocketPath {
    type Error = error::SocketPath;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::from_path_buf(path)
    }
}

impl TryFrom<&Path> for UnixSocketPath {
    type Error = error::SocketPath;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

impl TryFrom<String> for UnixSocketPath {
    type Error = error::SocketPath;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

impl TryFrom<&str> for UnixSocketPath {
    type Error = error::SocketPath;

    fn try_from(path: &str) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

impl From<UnixSocketPath> for PathBuf {
    fn from(path: UnixSocketPath) -> Self {
        path.inner
    }
}
