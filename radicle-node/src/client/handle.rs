use std::net;
use std::sync::Arc;

use crossbeam_channel as chan;
use nakamoto_net::Waker;
use thiserror::Error;

use crate::identity::Id;
use crate::service;
use crate::service::{CommandError, FetchLookup, QueryState};
use crate::service::{NodeId, Session};

/// An error resulting from a handle method.
#[derive(Error, Debug)]
pub enum Error {
    /// The command channel is no longer connected.
    #[error("command channel is not connected")]
    NotConnected,
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
        Self::NotConnected
    }
}

impl From<chan::RecvTimeoutError> for Error {
    fn from(err: chan::RecvTimeoutError) -> Self {
        match err {
            chan::RecvTimeoutError::Timeout => Self::Timeout,
            chan::RecvTimeoutError::Disconnected => Self::NotConnected,
        }
    }
}

impl<T> From<chan::SendError<T>> for Error {
    fn from(_: chan::SendError<T>) -> Self {
        Self::NotConnected
    }
}

pub struct Handle<W: Waker> {
    pub(crate) commands: chan::Sender<service::Command>,
    pub(crate) shutdown: chan::Sender<()>,
    pub(crate) listening: chan::Receiver<net::SocketAddr>,
    pub(crate) waker: W,
}

impl<W: Waker> traits::Handle for Handle<W> {
    fn listening(&self) -> Result<net::SocketAddr, Error> {
        self.listening.recv().map_err(Error::from)
    }

    fn fetch(&mut self, id: Id) -> Result<FetchLookup, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.commands.send(service::Command::Fetch(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn track_node(&mut self, id: NodeId, alias: Option<String>) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.commands
            .send(service::Command::TrackNode(id, alias, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.commands
            .send(service::Command::UntrackNode(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn track_repo(&mut self, id: Id) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.commands
            .send(service::Command::TrackRepo(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.commands
            .send(service::Command::UntrackRepo(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        self.command(service::Command::AnnounceRefs(id))
    }

    fn command(&self, cmd: service::Command) -> Result<(), Error> {
        self.commands.send(cmd)?;
        self.waker.wake()?;

        Ok(())
    }

    fn routing(&self) -> Result<chan::Receiver<(Id, NodeId)>, Error> {
        let (sender, receiver) = chan::unbounded();
        let query: Arc<QueryState> = Arc::new(move |state| {
            for (id, node) in state.routing().entries()? {
                if sender.send((id, node)).is_err() {
                    break;
                }
            }
            Ok(())
        });
        let (err_sender, err_receiver) = chan::bounded(1);
        self.command(service::Command::QueryState(query, err_sender))?;
        err_receiver.recv()??;

        Ok(receiver)
    }

    fn sessions(&self) -> Result<chan::Receiver<(NodeId, Session)>, Error> {
        // TODO: This can be implemented once we have real peer sessions.
        todo!()
    }

    fn inventory(&self) -> Result<chan::Receiver<Id>, Error> {
        let (sender, receiver) = chan::unbounded();
        let query: Arc<QueryState> = Arc::new(move |state| {
            for id in state.inventory()?.iter() {
                if sender.send(*id).is_err() {
                    break;
                }
            }
            Ok(())
        });
        let (err_sender, err_receiver) = chan::bounded(1);
        self.command(service::Command::QueryState(query, err_sender))?;
        err_receiver.recv()??;

        Ok(receiver)
    }

    fn shutdown(self) -> Result<(), Error> {
        self.shutdown.send(())?;
        self.waker.wake()?;

        Ok(())
    }
}

pub mod traits {
    use super::*;

    pub trait Handle {
        /// Wait for the node's listening socket to be bound.
        fn listening(&self) -> Result<net::SocketAddr, Error>;
        /// Retrieve or update the project from network.
        fn fetch(&mut self, id: Id) -> Result<FetchLookup, Error>;
        /// Start tracking the given project. Doesn't do anything if the project is already
        /// tracked.
        fn track_repo(&mut self, id: Id) -> Result<bool, Error>;
        /// Start tracking the given node.
        fn track_node(&mut self, id: NodeId, alias: Option<String>) -> Result<bool, Error>;
        /// Untrack the given project and delete it from storage.
        fn untrack_repo(&mut self, id: Id) -> Result<bool, Error>;
        /// Untrack the given node.
        fn untrack_node(&mut self, id: NodeId) -> Result<bool, Error>;
        /// Notify the client that a project has been updated.
        fn announce_refs(&mut self, id: Id) -> Result<(), Error>;
        /// Send a command to the command channel, and wake up the event loop.
        fn command(&self, cmd: service::Command) -> Result<(), Error>;
        /// Ask the client to shutdown.
        fn shutdown(self) -> Result<(), Error>;
        /// Query the routing table entries.
        fn routing(&self) -> Result<chan::Receiver<(Id, NodeId)>, Error>;
        /// Query the peer session state.
        fn sessions(&self) -> Result<chan::Receiver<(NodeId, Session)>, Error>;
        /// Query the inventory.
        fn inventory(&self) -> Result<chan::Receiver<Id>, Error>;
    }
}
