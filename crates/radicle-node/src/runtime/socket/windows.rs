use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use super::error;

const MAX_PATH_LEN: usize = 256;

/// The required prefix for local Windows named pipes.
const LOCAL_PIPE_PREFIX: &str = r"\\.\pipe\";

/// A validated local Windows named pipe path.
///
/// This type guarantees that the contained path:
/// - Follows the format `\\.\pipe\<pipename>`.
/// - Does not exceed a length of 256 characters.
/// - The `<pipename>` portion does not contain backslashes.
/// - Is valid UTF-8.
///
/// # Example
///
/// ```ignore, rust
/// // Create from just the pipe name (recommended)
/// let path = WindowsPipePath::new("myapp")?;
/// assert_eq!(path.as_str(), r"\\.\pipe\myapp");
///
/// // Or validate a full path
/// let path = WindowsPipePath::from_full_path(r"\\.\pipe\myapp")?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowsPipePath {
    inner: PathBuf,
    /// The pipe name portion (after `\\.\pipe\`).
    pipe_name: String,
}

impl WindowsPipePath {
    /// Creates a new [`WindowsPipePath`] from a pipe name.
    ///
    /// This automatically prepends `\\.\pipe\` to the name.
    ///
    /// # Example
    ///
    /// ```ignore, rust
    /// let path = WindowsPipePath::new("myapp")?;
    /// assert_eq!(path.as_str(), r"\\.\pipe\myapp");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The pipe name is empty
    /// - The pipe name contains backslashes
    /// - The resulting full path exceeds 256 characters
    /// - The pipe name contains null bytes
    pub fn new<S: AsRef<str>>(pipe_name: S) -> Result<Self, error::SocketPath> {
        let name = pipe_name.as_ref();
        Self::validate_pipe_name(name)?;

        let path = format!(r"\\.\pipe\{name}");
        let path_length = path.len();
        if path_length > MAX_PATH_LEN {
            return Err(error::SocketPath::PathTooLong {
                path,
                length: path_length,
                max: MAX_PATH_LEN,
            });
        }

        Ok(Self {
            inner: PathBuf::from(&path),
            pipe_name: name.to_string(),
        })
    }

    /// Creates a [`WindowsPipePath`] from a [`Path`].
    ///
    /// The path must be in the format `\\.\pipe\<pipename>`.
    ///
    /// # Example
    ///
    /// ```rust
    /// let path = WindowsPipePath::from_full_path(r"\\.\pipe\myapp")?;
    /// assert_eq!(path.pipe_name(), "myapp");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is empty
    /// - The path doesn't start with `\\.\pipe\`
    /// - The path exceeds 256 characters
    /// - The pipe name portion is empty or contains backslashes
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, error::SocketPath> {
        let path = path.as_ref();
        let path_str = path.to_str().ok_or(error::SocketPath::InvalidUtf8 {
            path: path.into_path_buf(),
        })?;

        if path_str.is_empty() {
            return Err(error::SocketPath::EmptyPath);
        }

        if path_str.len() > MAX_PATH_LEN {
            return Err(error::SocketPath::PathTooLong {
                length: path_str.len(),
                max: MAX_PATH_LEN,
            });
        }

        // Normalize forward slashes to backslashes for comparison
        let normalized = path_str.replace('/', "\\");

        let pipe_name = normalized
            .to_lowercase()
            .strip_prefix(LOCAL_PIPE_PREFIX.to_lowercase())
            .ok_or(error::SocketPath::InvalidPipeFormat {
                reason: "path must start with \\\\.\\pipe\\",
            })?;

        Self::validate_pipe_name(pipe_name)?;

        Ok(Self {
            inner: path.to_path_buf(),
            pipe_name: pipe_name.to_string(),
        })
    }

    fn validate_pipe_name(name: &str) -> Result<(), error::SocketPath> {
        if name.is_empty() {
            return Err(error::SocketPath::InvalidPipeFormat {
                reason: "pipe name cannot be empty",
            });
        }

        if name.contains(['\\', '/']) {
            return Err(error::SocketPath::PipeNameContainsBackslash {
                pipe_name: name.to_string(),
            });
        }

        if let Some(pos) = name.bytes().position(|b| b == 0) {
            return Err(error::SocketPath::ContainsNullByte { position: pos });
        }

        Ok(())
    }

    /// Returns the full path as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        // Safe because we validated UTF-8 during construction
        self.inner.to_str().unwrap()
    }

    /// Returns the full path as a [`Path`] reference.
    #[inline]
    pub fn as_path(&self) -> &Path {
        &self.inner
    }

    /// Returns just the pipe name, without the `\\.\pipe\` prefix.
    #[inline]
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    /// Consumes the [`WindowsPipePath`] and returns the inner [`PathBuf`].
    #[inline]
    pub fn into_path_buf(self) -> PathBuf {
        self.inner
    }
}

impl AsRef<Path> for WindowsPipePath {
    #[inline]
    fn as_ref(&self) -> &Path {
        &self.inner
    }
}

impl AsRef<OsStr> for WindowsPipePath {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        self.inner.as_os_str()
    }
}

impl AsRef<str> for WindowsPipePath {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for WindowsPipePath {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl fmt::Display for WindowsPipePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<PathBuf> for WindowsPipePath {
    type Error = error::SocketPath;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::from_path(path)
    }
}

impl TryFrom<&Path> for WindowsPipePath {
    type Error = error::SocketPath;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        Self::from_path(path)
    }
}

impl TryFrom<String> for WindowsPipePath {
    type Error = error::SocketPath;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        Self::from_path(path)
    }
}

impl TryFrom<&str> for WindowsPipePath {
    type Error = error::SocketPath;

    fn try_from(path: &str) -> Result<Self, Self::Error> {
        Self::from_path(path)
    }
}

impl From<WindowsPipePath> for PathBuf {
    fn from(path: WindowsPipePath) -> Self {
        path.inner
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_creates_full_path() {
        let path = WindowsPipePath::new("myapp").unwrap();
        assert_eq!(path.as_str(), r"\\.\pipe\myapp");
        assert_eq!(path.pipe_name(), "myapp");
    }

    #[test]
    fn test_from_full_path_valid() {
        let path = WindowsPipePath::from_path(r"\\.\pipe\myapp").unwrap();
        assert_eq!(path.pipe_name(), "myapp");
    }

    #[test]
    fn test_from_full_path_with_forward_slashes() {
        let path = WindowsPipePath::from_path("//./pipe/myapp").unwrap();
        assert_eq!(path.pipe_name(), "myapp");
    }

    #[test]
    fn test_empty_name_rejected() {
        let result = WindowsPipePath::new("");
        assert!(matches!(
            result,
            Err(error::SocketPath::InvalidPipeFormat { .. })
        ));
    }

    #[test]
    fn test_empty_path_rejected() {
        let result = WindowsPipePath::from_path("");
        assert!(matches!(result, Err(error::SocketPath::EmptyPath)));
    }

    #[test]
    fn test_invalid_prefix_rejected() {
        let result = WindowsPipePath::from_path(r"C:\temp\pipe");
        assert!(matches!(
            result,
            Err(error::SocketPath::InvalidPipeFormat { .. })
        ));
    }

    #[test]
    fn test_remote_pipe_rejected() {
        let result = WindowsPipePath::from_path(r"\\server\pipe\myapp");
        assert!(matches!(
            result,
            Err(error::SocketPath::InvalidPipeFormat { .. })
        ));
    }

    #[test]
    fn test_pipe_name_with_backslash_rejected() {
        let result = WindowsPipePath::new(r"my\app");
        assert!(matches!(
            result,
            Err(error::SocketPath::PipeNameContainsBackslash)
        ));
    }

    #[test]
    fn test_pipe_name_with_forward_slash_rejected() {
        let result = WindowsPipePath::new("my/app");
        assert!(matches!(
            result,
            Err(error::SocketPath::PipeNameContainsBackslash)
        ));
    }

    #[test]
    fn test_path_too_long() {
        let long_name = "x".repeat(WindowsPipePath::max_pipe_name_chars() + 1);
        let result = WindowsPipePath::new(&long_name);
        assert!(matches!(result, Err(error::SocketPath::PathTooLong { .. })));
    }

    #[test]
    fn test_max_length_name_accepted() {
        let max_name = "x".repeat(WindowsPipePath::max_pipe_name_chars());
        let result = WindowsPipePath::new(&max_name);
        assert!(result.is_ok());
    }

    #[test]
    fn test_display() {
        let path = WindowsPipePath::new("myapp").unwrap();
        assert_eq!(format!("{}", path), r"\\.\pipe\myapp");
    }

    #[test]
    fn test_max_pipe_name_chars() {
        // Prefix is "\\.\pipe\" = 9 characters
        assert_eq!(WindowsPipePath::max_pipe_name_chars(), 256 - 9);
    }
}
