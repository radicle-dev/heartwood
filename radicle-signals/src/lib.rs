use std::io;
use std::sync::Mutex;

use crossbeam_channel as chan;

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

impl TryFrom<i32> for Signal {
    type Error = i32;

    #[cfg(not(windows))]
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            libc::SIGTERM => Ok(Self::Terminate),
            libc::SIGINT => Ok(Self::Interrupt),
            libc::SIGWINCH => Ok(Self::WindowChanged),
            libc::SIGHUP => Ok(Self::Hangup),
            _ => Err(value),
        }
    }

    #[cfg(windows)]
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            libc::SIGTERM => Ok(Self::Terminate),
            libc::SIGINT => Ok(Self::Interrupt),
            _ => Err(value),
        }
    }
}

/// Signal notifications are sent via this channel.
#[cfg(not(windows))]
static NOTIFY: Mutex<Option<chan::Sender<Signal>>> = Mutex::new(None);

/// Install global signal handlers.
#[cfg(not(windows))]
pub fn install(notify: chan::Sender<Signal>) -> io::Result<()> {
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

/// Install global signal handlers.
///
/// # Safety
///
/// Calls `libc` functions safely.
#[cfg(not(windows))]
unsafe fn _install() -> io::Result<()> {
    if libc::signal(libc::SIGTERM, handler as libc::sighandler_t) == libc::SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    if libc::signal(libc::SIGINT, handler as libc::sighandler_t) == libc::SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    if libc::signal(libc::SIGHUP, handler as libc::sighandler_t) == libc::SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    if libc::signal(libc::SIGWINCH, handler as libc::sighandler_t) == libc::SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Called by `libc` when a signal is received.
#[cfg(not(windows))]
extern "C" fn handler(sig: libc::c_int, _info: *mut libc::siginfo_t, _data: *mut libc::c_void) {
    let Ok(sig) = sig.try_into() else {
        return;
    };
    if let Ok(guard) = NOTIFY.try_lock() {
        if let Some(c) = &*guard {
            c.try_send(sig).ok();
        }
    }
}

#[cfg(windows)]
pub fn install(_: chan::Sender<Signal>) -> io::Result<()> {
    Ok(())
}