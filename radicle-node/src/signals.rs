use std::io;
use std::sync::Mutex;

use crossbeam_channel as chan;

/// Signal notifications are sent via this channel.
static NOTIFY: Mutex<Option<chan::Sender<()>>> = Mutex::new(None);

/// Install global signal handlers for `SIGTERM` and `SIGINT`.
pub fn install(notify: chan::Sender<()>) -> io::Result<()> {
    if let Ok(mut channel) = NOTIFY.try_lock() {
        if channel.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "signal handler is already installed",
            ));
        }
        *channel = Some(notify);

        unsafe { _install() }?;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "unable to install signal handler",
        ));
    }
    Ok(())
}

/// Install global signal handlers for `SIGTERM` and `SIGINT`.
///
/// # Safety
///
/// Calls `libc` functions safely.
unsafe fn _install() -> io::Result<()> {
    if libc::signal(libc::SIGTERM, handler as libc::sighandler_t) == libc::SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    if libc::signal(libc::SIGINT, handler as libc::sighandler_t) == libc::SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Called by `libc` when a signal is received.
extern "C" fn handler(sig: libc::c_int, _info: *mut libc::siginfo_t, _data: *mut libc::c_void) {
    if sig != libc::SIGTERM && sig != libc::SIGINT {
        return;
    }
    if let Ok(guard) = NOTIFY.try_lock() {
        if let Some(c) = &*guard {
            c.try_send(()).ok();
        }
    }
}
