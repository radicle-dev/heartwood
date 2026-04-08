use std::io;
use std::sync::OnceLock;

use crossbeam_channel as chan;

use ::windows::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    SetConsoleCtrlHandler,
};
use ::windows::core::BOOL;

use crate::{Signal, already_installed};

static NOTIFY: OnceLock<chan::Sender<Signal>> = OnceLock::new();

/// Callback function, called by the system when a control signal is to be received.
/// See <https://learn.microsoft.com/en-us/windows/console/handlerroutine>.
unsafe extern "system" fn handler(ctrltype: u32) -> BOOL {
    match ctrltype {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_SHUTDOWN_EVENT => {
            if let Some(notify) = NOTIFY.get() {
                if notify.send(Signal::Terminate).is_ok() {
                    return true.into();
                }
            } else {
                // Do nothing, since we do not have a channel to send notifications to.
            }
        }
        CTRL_LOGOFF_EVENT => {
            // Do nothing, since we do not know which user is logging off.
        }
        _ => {
            // Do nothing, since we received an unknown control signal.
        }
    }

    false.into()
}

/// Install global signal handlers, with notifications sent to the given
/// `notify` channel.
pub fn install(notify: chan::Sender<Signal>) -> io::Result<()> {
    if let Err(_) = NOTIFY.set(notify) {
        return Err(already_installed());
    }

    // SAFETY: Our handler function is sane.
    let result = unsafe { SetConsoleCtrlHandler(Some(handler), true) };
    result.map_err(|_| io::Error::last_os_error())
}
