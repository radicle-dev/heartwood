use std::ops::Deref;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{fmt, io, time};

use crossbeam_channel as chan;
use radicle::node::Seeds;
use thiserror::Error;

use crate::identity::Id;
use crate::node::{Command, FetchResult};
use crate::profile::Home;
use crate::runtime::Emitter;
use crate::service;
use crate::service::tracking;
use crate::service::Event;
use crate::service::{CommandError, QueryState};
use crate::service::{NodeId, Sessions};
use crate::wire;
use crate::wire::StreamId;
use crate::worker::TaskResult;

/// An error resulting from a handle method.
#[derive(Error, Debug)]
pub enum Error {
    /// The command channel is no longer connected.
    #[error("command channel is not connected")]
    ChannelDisconnected,
    /// The command returned an error.
    #[error("command failed: {0}")]
    Command(#[from] CommandError),
    /// The operation timed out.
    #[error("the operation timed out")]
    Timeout,
    /// An I/O error occured.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<chan::RecvError> for Error {
    fn from(_: chan::RecvError) -> Self {
        Self::ChannelDisconnected
    }
}

impl From<chan::RecvTimeoutError> for Error {
    fn from(err: chan::RecvTimeoutError) -> Self {
        match err {
            chan::RecvTimeoutError::Timeout => Self::Timeout,
            chan::RecvTimeoutError::Disconnected => Self::ChannelDisconnected,
        }
    }
}

impl<T> From<chan::SendError<T>> for Error {
    fn from(_: chan::SendError<T>) -> Self {
        Self::ChannelDisconnected
    }
}

pub struct Handle {
    pub(crate) home: Home,
    pub(crate) controller: reactor::Controller<wire::Control>,

    /// Whether a shutdown was initiated or not. Prevents attempting to shutdown twice.
    shutdown: Arc<AtomicBool>,
    /// Publishes events to subscribers.
    emitter: Emitter<Event>,
}

/// Events feed.
pub struct Events(chan::Receiver<Event>);

impl Deref for Events {
    type Target = chan::Receiver<Event>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Events {
    /// Listen for events, and wait for the given predicate to return something,
    /// or timeout if the specified amount of time has elapsed.
    pub fn wait<F>(
        &self,
        mut f: F,
        timeout: time::Duration,
    ) -> Result<Event, chan::RecvTimeoutError>
    where
        F: FnMut(&Event) -> bool,
    {
        let start = time::Instant::now();

        loop {
            if let Some(timeout) = timeout.checked_sub(start.elapsed()) {
                match self.recv_timeout(timeout) {
                    Ok(event) => {
                        if f(&event) {
                            return Ok(event);
                        }
                    }
                    Err(err @ chan::RecvTimeoutError::Disconnected) => {
                        return Err(err);
                    }
                    Err(chan::RecvTimeoutError::Timeout) => {
                        // Keep trying until our timeout reaches zero.
                        continue;
                    }
                }
            } else {
                return Err(chan::RecvTimeoutError::Timeout);
            }
        }
    }
}

impl Handle {
    /// Subscribe to events stream.
    pub fn events(&self) -> Events {
        Events(self.emitter.subscribe())
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handle").field("home", &self.home).finish()
    }
}

impl Clone for Handle {
    fn clone(&self) -> Self {
        Self {
            home: self.home.clone(),
            controller: self.controller.clone(),
            shutdown: self.shutdown.clone(),
            emitter: self.emitter.clone(),
        }
    }
}

impl Handle {
    pub fn new(
        home: Home,
        controller: reactor::Controller<wire::Control>,
        emitter: Emitter<Event>,
    ) -> Self {
        Self {
            home,
            controller,
            shutdown: Arc::default(),
            emitter,
        }
    }

    pub fn worker_result(&mut self, result: TaskResult) -> Result<(), io::Error> {
        self.controller.cmd(wire::Control::Worker(result))
    }

    pub fn flush(&mut self, remote: NodeId, stream: StreamId) -> Result<(), io::Error> {
        self.controller.cmd(wire::Control::Flush { remote, stream })
    }

    fn command(&self, cmd: service::Command) -> Result<(), io::Error> {
        self.controller.cmd(wire::Control::User(cmd))
    }
}

impl radicle::node::Handle for Handle {
    type Sessions = Sessions;
    type Error = Error;

    fn is_running(&self) -> bool {
        true
    }

    fn connect(&mut self, node: NodeId, addr: radicle::node::Address) -> Result<(), Error> {
        self.command(service::Command::Connect(node, addr))?;

        Ok(())
    }

    fn seeds(&mut self, id: Id) -> Result<Seeds, Self::Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Seeds(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn fetch(&mut self, id: Id, from: NodeId) -> Result<FetchResult, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Fetch(id, from, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn track_node(&mut self, id: NodeId, alias: Option<String>) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::TrackNode(id, alias, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::UntrackNode(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn track_repo(&mut self, id: Id, scope: tracking::Scope) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::TrackRepo(id, scope, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::UntrackRepo(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        self.command(service::Command::AnnounceRefs(id))
            .map_err(Error::from)
    }

    fn announce_inventory(&mut self) -> Result<(), Error> {
        self.command(service::Command::AnnounceInventory)
            .map_err(Error::from)
    }

    fn sync_inventory(&mut self) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::SyncInventory(sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn sessions(&self) -> Result<Self::Sessions, Error> {
        let (sender, receiver) = chan::unbounded();
        let query: Arc<QueryState> = Arc::new(move |state| {
            sender.send(state.sessions().clone()).ok();
            Ok(())
        });
        let (err_sender, err_receiver) = chan::bounded(1);
        self.command(service::Command::QueryState(query, err_sender))?;
        err_receiver.recv()??;

        let sessions = receiver.recv()?;

        Ok(sessions)
    }

    fn shutdown(self) -> Result<(), Error> {
        // If the current value is `false`, set it to `true`, otherwise error.
        if self
            .shutdown
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        // Send a shutdown request to our own control socket. This is the only way to kill the
        // control thread gracefully. Since the control thread may have called this function,
        // the control socket may already be disconnected. Ignore errors.
        UnixStream::connect(self.home.socket())
            .and_then(|sock| Command::SHUTDOWN.to_writer(sock))
            .ok();

        self.controller
            .shutdown()
            .map_err(|_| Error::ChannelDisconnected)
    }
}
