use std::sync::Arc;

use crossbeam_channel as chan;
use thiserror::Error;

use crate::identity::Id;
use crate::service;
use crate::service::{CommandError, FetchLookup, QueryState};
use crate::service::{NodeId, Sessions};

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

pub struct Handle<T: reactor::Handler> {
    pub(crate) controller: reactor::Controller<T>,
}

impl<T: reactor::Handler> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
        }
    }
}

impl<T: reactor::Handler> From<reactor::Controller<T>> for Handle<T> {
    fn from(controller: reactor::Controller<T>) -> Handle<T> {
        Handle { controller }
    }
}

impl<T: reactor::Handler<Command = service::Command>> Handle<T> {
    fn command(&self, cmd: service::Command) -> Result<(), Error> {
        self.controller.send(cmd)?;
        Ok(())
    }
}

impl<T: reactor::Handler<Command = service::Command>> radicle::node::Handle for Handle<T> {
    type Sessions = Sessions;
    type FetchLookup = FetchLookup;
    type Error = Error;

    fn connect(&mut self, node: NodeId, addr: radicle::node::Address) -> Result<(), Error> {
        self.command(service::Command::Connect(node, addr))?;

        Ok(())
    }

    fn fetch(&mut self, id: Id) -> Result<Self::FetchLookup, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::Fetch(id, sender))?;
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

    fn track_repo(&mut self, id: Id) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::TrackRepo(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Error> {
        let (sender, receiver) = chan::bounded(1);
        self.command(service::Command::UntrackRepo(id, sender))?;
        receiver.recv().map_err(Error::from)
    }

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        self.command(service::Command::AnnounceRefs(id))
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
        self.controller.shutdown().map_err(|_| Error::NotConnected)
    }
}
