use std::fmt;
use std::io;

use libc::{getrlimit, rlimit, setrlimit, RLIMIT_NOFILE};

#[cfg(target_family = "unix")]
/// Sets the open file limit to the given value, or the maximum allowed value.
pub fn set_file_limit<N>(n: N) -> io::Result<u64>
where
    N: Copy + fmt::Display,
    u64: TryFrom<N>,
{
    let Ok(n) = u64::try_from(n) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected value that fits into u64, found: {n}"),
        ));
    };
    let mut rlim = rlimit {
        rlim_cur: 0, // Initial soft limit value
        rlim_max: 0, // Initial hard limit value
    };
    // Get the current limits.
    unsafe {
        if getrlimit(RLIMIT_NOFILE, &mut rlim) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    if rlim.rlim_cur >= n {
        return Ok(rlim.rlim_cur);
    }
    // Set the soft limit to the given value, up to the hard limit.
    rlim.rlim_cur = n.min(rlim.rlim_max);
    unsafe {
        if setrlimit(RLIMIT_NOFILE, &rlim as *const rlimit) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(rlim.rlim_cur)
}

// N.b. windows uses i64 instead of u64
#[cfg(target_family = "windows")]
/// Sets the open file limit to the given value, or the maximum allowed value.
pub fn set_file_limit<N>(n: N) -> io::Result<i64>
where
    N: Copy + fmt::Display,
    i64: TryFrom<N>,
{
    let Ok(n) = u64::try_from(n) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected value that fits into i64, found: {n}"),
        ));
    };
    let mut rlim = rlimit {
        rlim_cur: 0, // Initial soft limit value
        rlim_max: 0, // Initial hard limit value
    };
    // Get the current limits.
    unsafe {
        if getrlimit(RLIMIT_NOFILE, &mut rlim) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    if rlim.rlim_cur >= n {
        return Ok(rlim.rlim_cur);
    }
    // Set the soft limit to the given value, up to the hard limit.
    rlim.rlim_cur = n.min(rlim.rlim_max);
    unsafe {
        if setrlimit(RLIMIT_NOFILE, &rlim as *const rlimit) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(rlim.rlim_cur)
}
