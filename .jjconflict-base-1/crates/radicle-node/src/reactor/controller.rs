use crossbeam_channel::Sender;
use mio::Waker;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;

use crate::wire;

/// A command which may be sent to the [`super::ReactionHandler`] from outside of the [`super::Reactor`],
/// including other threads.
///
/// The handler object is owned by the reactor runtime and executes always in the context of the
/// reactor runtime thread. Thus, if other (micro)services within the app needs to communicate
/// to the handler they have to use this data type, which usually is an enumeration for a set of
/// commands supported by the handler.
pub enum ControlMessage {
    Command(Box<wire::Control>),
    Shutdown,
}

/// Used by the [`crate::reactor::Reactor`] to inform the
/// [`crate::reactor::ReactionHandler`] about
/// incoming commands, sent via this [`Controller`].
#[derive(Clone)]
pub struct Controller {
    sender: Sender<ControlMessage>,
    waker: Arc<Waker>,
}

impl Controller {
    pub fn new(sender: Sender<ControlMessage>, waker: Arc<Waker>) -> Self {
        Self { sender, waker }
    }

    pub fn wake(&self) -> io::Result<()> {
        log::trace!(target: "reactor::controller", "Wakening the reactor");
        self.waker.wake()
    }

    pub fn cmd(&self, command: wire::Control) -> io::Result<()> {
        log::trace!(target: "reactor::controller", "Sending command {command:?} to the reactor");
        self.sender
            .send(ControlMessage::Command(Box::new(command)))
            .map_err(|_| ErrorKind::BrokenPipe)?;
        self.wake()
    }

    pub fn shutdown(self) -> Result<(), Self> {
        log::info!(target: "reactor::controller", "Initiating reactor shutdown...");
        let res1 = self.sender.send(ControlMessage::Shutdown);
        let res2 = self.wake();
        res1.or(res2).map_err(|_| self)
    }
}
