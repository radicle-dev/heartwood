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

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            libc::SIGTERM => Ok(Self::Terminate),
            libc::SIGINT => Ok(Self::Interrupt),
            libc::SIGWINCH => Ok(Self::WindowChanged),
            libc::SIGHUP => Ok(Self::Hangup),
            _ => Err(value),
        }
    }
}

/// Signal notifications are sent via this channel.
static NOTIFY: Mutex<Option<chan::Sender<Signal>>> = Mutex::new(None);

/// A slice of signals to handle.
const SIGNALS: &[i32] = &[libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGWINCH];

/// Install global signal handlers.
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

/// Uninstall global signal handlers.
pub fn uninstall() -> io::Result<()> {
    if let Ok(mut channel) = NOTIFY.try_lock() {
        if channel.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "signal handler is already uninstalled",
            ));
        }
        *channel = None;

        unsafe { _uninstall() }?;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "unable to uninstall signal handler",
        ));
    }
    Ok(())
}

/// Install global signal handlers.
///
/// # Safety
///
/// Calls `libc` functions safely.
unsafe fn _install() -> io::Result<()> {
    for signal in SIGNALS {
        if libc::signal(*signal, handler as libc::sighandler_t) == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Uninstall global signal handlers.
///
/// # Safety
///
/// Calls `libc` functions safely.
unsafe fn _uninstall() -> io::Result<()> {
    for signal in SIGNALS {
        if libc::signal(*signal, libc::SIG_DFL) == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Called by `libc` when a signal is received.
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
