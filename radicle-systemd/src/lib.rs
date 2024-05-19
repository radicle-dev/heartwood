//! Library for interaction with systemd, specialized for Radicle.

use std::env::{remove_var, var, VarError};
use std::os::fd::RawFd;
use std::process::id;

const LISTEN_PID: &str = "LISTEN_PID";
const LISTEN_FDS: &str = "LISTEN_FDS";
const LISTEN_FDNAMES: &str = "LISTEN_FDNAMES";

/// Minimum file descriptor used by systemd.
/// See <https://github.com/systemd/systemd/blob/v254/src/systemd/sd-daemon.h#L56>.
const SD_LISTEN_FDS_START: RawFd = 3;

/// Checks whether *at most one* file descriptor with given name was passed, returning it.
/// systemd sending none, more than one, or a file descriptor with a different name, all
/// results in [`Option::None`], but errors decoding environment variables or missing
/// environment variables will error.
/// This is a specialization of [`sd_listen_fds_with_names(3)`](man:sd_listen_fds_with_names(3)).
/// See:
///  - <https://www.freedesktop.org/software/systemd/man/254/sd_listen_fds_with_names.html>
///  - <https://github.com/systemd/systemd/blob/v254/src/libsystemd/sd-daemon/sd-daemon.c>
///  - <https://0pointer.de/blog/projects/socket-activation.html>
///  - <https://0pointer.de/blog/projects/socket-activation2.html>
pub fn listen_fd(name: &str) -> Result<Option<RawFd>, VarError> {
    let fd = match var(LISTEN_PID) {
        Err(VarError::NotPresent) => Ok(None),
        Err(err) => Err(err),
        Ok(pid) if pid != id().to_string() => Ok(None),
        _ if var(LISTEN_FDS)? != "1" || var(LISTEN_FDNAMES).ok() != Some(name.to_string()) => {
            Ok(None)
        }
        _ => Ok(Some(SD_LISTEN_FDS_START)),
    };

    remove_var(LISTEN_PID);
    remove_var(LISTEN_FDS);
    remove_var(LISTEN_FDNAMES);

    fd
}
