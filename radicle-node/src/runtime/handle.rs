use std::net;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{fmt, io, time};

use crossbeam_channel as chan;
use radicle::node::{ConnectOptions, ConnectResult, Link, Seeds};
use radicle::storage::refs::RefsAt;
use reactor::poller::popol::PopolWaker;
use thiserror::Error;

use crate::identity::RepoId;
use crate::node::{Alias, Command, FetchResult};
use crate::profile::Home;
use crate::runtime::Emitter;
use crate::service;
use crate::service::policy;
use crate::service::NodeId;
use crate::service::{CommandError, Config, QueryState};
use crate::service::{Event, Events};
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
    pub(crate) controller: reactor::Controller<wire::Control, PopolWaker>,

    /// Whether a shutdown was initiated or not. Prevents attempting to shutdown twice.
    shutdown: Arc<AtomicBool>,
    /// Publishes events to subscribers.
    emitter: Emitter<Event>,
}

impl Handle {
    /// Subscribe to events stream.
    pub fn events(&self) -> Events {
        Events::from(self.emitter.subscribe())
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
        controller: reactor::Controller<wire::Control, PopolWaker>,
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

    pub(crate) fn command(&self, cmd: service::Command) -> Result<(), io::Error> {
        self.controller.cmd(wire::Control::User(cmd))
    }
}

impl radicle::node::Handle for Handle {
    type Sessions = Vec<radicle::node::Session>;
    type Error = Error;

    fn nid(&self) -> Result<NodeId, Self::Error> {
        let (sender, receiver) = chan::bounded(1);
        let query: Arc<QueryState> = Arc::new(move |state| {
            sender.send(*state.nid()).ok();
            Ok(())
        });
        let (err_sender, err_receiver) = chan::bounded(1);
        self.command(service::Command::QueryState(query, err_sender))?;
        err_receiver.recv()??;

        let nid = receiver.recv()?;

        Ok(nid)
    }

    fn is_running(&self) -> bool {
        true
    }

    fn connect(
        &mut self,
        node: NodeId,
        addr: radicle::node::Address,
        opts: ConnectOptions,
    ) -> Result<ConnectResult, Error> {
        let events = self.events();
        let timeout = opts.timeout;
        let sessions = self.sessions()?;
        let session = sessions.iter().find(|s| s.nid == node);

        if let Some(s) = session {
            if s.state.is_connected() {
                return Ok(ConnectResult::Connected);
            }
        }
        self.command(service::Command::Connect(node, addr, opts))?;

        events
            .wait(
                |e| match e {
                    Event::PeerConnected { nid } if nid == &node => Some(ConnectResult::Connected),
                    Event::PeerDisconnected { nid, reason } if nid == &node => {
                        Some(ConnectResult::Disconnected {
                            reason: reason.clone(),
                        })
                    }
                    _ => None,
                },
                timeout,
            )
            .map_err(Error::from)
    }

    fn disconnect(&mut self, node: NodeId) -> Result<(), Self::Error> {
        let events = self.events();
        self.command(service::Command::Disconnect(node))?;
        events
            .wait(
                |e| match e {
                    Event::PeerDisconnected { nid, .. } if nid == &node => Some(()),
                    _ => None,
                },
                time::Duration::MAX,
            )
            .map_err(Error::from)
    }

    fn seeds(&mut self, id: RepoId) -> Result<Seeds, Self::Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Seeds(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn config(&self) -> Result<Config, Self::Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Config(sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn listen_addrs(&self) -> Result<Vec<net::SocketAddr>, Self::Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::ListenAddrs(sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn fetch(
        &mut self,
        id: RepoId,
        from: NodeId,
        timeout: time::Duration,
    ) -> Result<FetchResult, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Fetch(id, from, timeout, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn follow(&mut self, id: NodeId, alias: Option<Alias>) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Follow(id, alias, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn unfollow(&mut self, id: NodeId) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Unfollow(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn seed(&mut self, id: RepoId, scope: policy::Scope) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Seed(id, scope, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn unseed(&mut self, id: RepoId) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Unseed(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn announce_refs(&mut self, id: RepoId) -> Result<RefsAt, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::AnnounceRefs(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn announce_inventory(&mut self) -> Result<(), Error> {
        self.command(service::Command::AnnounceInventory)
            .map_err(Error::from)
    }

    fn update_inventory(&mut self, rid: RepoId) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::UpdateInventory(rid, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn subscribe(
        &self,
        _timeout: time::Duration,
    ) -> Result<Box<dyn Iterator<Item = Result<Event, Error>>>, Error> {
        Ok(Box::new(self.events().into_iter().map(Ok)))
    }

    fn sessions(&self) -> Result<Self::Sessions, Error> {
        let (sender, receiver) = chan::unbounded();
        let query: Arc<QueryState> = Arc::new(move |state| {
            let sessions = state
                .sessions()
                .iter()
                .map(|(nid, s)| radicle::node::Session {
                    nid: *nid,
                    link: if s.link.is_inbound() {
                        Link::Inbound
                    } else {
                        Link::Outbound
                    },
                    addr: s.addr.clone(),
                    state: s.state.clone(),
                })
                .collect();
            sender.send(sessions).ok();

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
            .and_then(|sock| Command::Shutdown.to_writer(sock))
            .ok();

        self.controller
            .shutdown()
            .map_err(|_| Error::ChannelDisconnected)
    }
}
