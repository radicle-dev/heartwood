use std::io;

use libc::{getrlimit, rlimit, setrlimit, RLIMIT_NOFILE};

/// Sets the open file limit to the given value, or the maximum allowed value.
pub fn set_file_limit(n: u64) -> io::Result<u64> {
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
    // Set the soft limit to the given value, up to the hard limit.
    rlim.rlim_cur = n.min(rlim.rlim_max);
    unsafe {
        if setrlimit(RLIMIT_NOFILE, &rlim as *const rlimit) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(rlim.rlim_cur)
}
