use std::io;

use crossbeam_channel as chan;
use signals_receipts::channel_notify_facility::{
    self, FinishError, InstallError, SendError, SignalsChannel as _, UninstallError,
};
use signals_receipts::SignalNumber;

use crate::channel_notify_facility_premade::SignalsChannel;

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

impl TryFrom<SignalNumber> for Signal {
    type Error = SignalNumber;

    fn try_from(value: SignalNumber) -> Result<Self, Self::Error> {
        match value {
            libc::SIGTERM => Ok(Self::Terminate),
            libc::SIGINT => Ok(Self::Interrupt),
            libc::SIGWINCH => Ok(Self::WindowChanged),
            libc::SIGHUP => Ok(Self::Hangup),
            _ => Err(value),
        }
    }
}

// The signals of interest to handle.
signals_receipts::channel_notify_facility! {
    SIGINT
    SIGTERM
    SIGHUP
    SIGWINCH
}

/// Install global signal handlers, with notifications sent to the given
/// `notify` channel.
pub fn install(notify: chan::Sender<Signal>) -> io::Result<()> {
    /// The sender type must implement the facility's trait.
    #[derive(Debug)]
    struct ChanSender(chan::Sender<Signal>);

    /// This also does our desired conversion from signal numbers to our
    /// `Signal` representation.
    impl channel_notify_facility::Sender for ChanSender {
        fn send(&self, sig_num: SignalNumber) -> Result<(), SendError> {
            if let Ok(sig) = sig_num.try_into() {
                self.0.send(sig).or(Err(SendError::Disconnected))
            } else {
                debug_assert!(false, "only called for recognized signal numbers");
                // Unrecognized signal numbers would be ignored, but
                // this never occurs.
                Err(SendError::Ignored)
            }
        }
    }

    SignalsChannel::install_with_outside_channel(ChanSender(notify)).map_err(|e| match e {
        InstallError::AlreadyInstalled { unused_notify: _ } => io::Error::new(
            io::ErrorKind::AlreadyExists,
            "signal handling is already installed",
        ),
        _ => io::Error::other(e), // The error type is non-exhaustive.
    })
}

/// Uninstall global signal handlers.
///
/// The caller must ensure that all `Receiver`s for the other end of the
/// channel are dropped to disconnect the channel, to ensure that the
/// internal "signals-receipt" thread wakes to clean-up (in case it's
/// blocked on sending on the channel).  Such dropping usually occurs
/// naturally when uninstalling, since the other end is no longer needed
/// then, and may be done after calling this.  Not doing so might
/// deadlock the "signals-receipt" thread.
pub fn uninstall() -> io::Result<()> {
    SignalsChannel::uninstall_with_outside_channel().map_err(|e| match e {
        UninstallError::AlreadyUninstalled => io::Error::new(
            io::ErrorKind::NotFound,
            "signal handling is already uninstalled",
        ),
        #[allow(clippy::unreachable)]
        UninstallError::WrongMethod => {
            // SAFETY: Impossible, because `SignalsChannel` is private
            // and so `SignalsChannel::install()` is never done.
            unreachable!()
        }
        _ => io::Error::other(e), // The error type is non-exhaustive.
    })
}

/// Do [`uninstall()`], terminate the internal "signals-receipt"
/// thread, and wait for that thread to finish.
///
/// This is provided in case it's ever needed to completely clean-up the
/// facility to be like it hadn't been installed before.  It's
/// unnecessary to use this, just to uninstall the handling.  Usually,
/// only using `uninstall` is desirable so that the "signals-receipt"
/// thread is kept alive for faster reuse when re-installing the
/// handling.
///
/// The caller must ensure that all `Receiver`s for the other end of the
/// channel have **already** been dropped to disconnect the channel,
/// before calling this, to ensure that the "signals-receipt" thread
/// wakes (in case it's blocked on sending on the channel) to see that
/// it must finish.  If this is not done, this might deadlock.
pub fn finish() -> io::Result<()> {
    SignalsChannel::finish_with_outside_channel().map_err(|e| match e {
        FinishError::AlreadyFinished => io::Error::new(
            io::ErrorKind::NotFound,
            "signal-handling facility is already finished",
        ),
        #[allow(clippy::unreachable)]
        FinishError::WrongMethod => {
            // SAFETY: Impossible, because `SignalsChannel` is private
            // and so `SignalsChannel::install()` is never done.
            unreachable!()
        }
        _ => io::Error::other(e), // The error type is non-exhaustive.
    })
}
