use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use crossbeam_channel as chan;
use cyphernet::EcSign;
use thiserror::Error;

use crate::crypto::Signer;
use crate::identity::Id;
use crate::profile::Home;
use crate::service;
use crate::service::{CommandError, FetchLookup, QueryState};
use crate::service::{NodeId, Sessions};
use crate::wire;
use crate::worker::WorkerResp;

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

pub struct Handle<G: Signer + EcSign> {
    pub(crate) home: Home,
    pub(crate) controller: reactor::Controller<wire::Control<G>>,
}

impl<G: Signer + EcSign> Clone for Handle<G> {
    fn clone(&self) -> Self {
        Self {
            home: self.home.clone(),
            controller: self.controller.clone(),
        }
    }
}

impl<G: Signer + EcSign + 'static> Handle<G> {
    pub fn new(home: Home, controller: reactor::Controller<wire::Control<G>>) -> Self {
        Self { home, controller }
    }

    pub fn worker_result(&mut self, resp: WorkerResp<G>) -> Result<(), Error> {
        match self.controller.cmd(wire::Control::Worker(resp)) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Err(Error::NotConnected),
            Err(err) => return Err(err.into()),
        }
        Ok(())
    }

    fn command(&self, cmd: service::Command) -> Result<(), Error> {
        self.controller.cmd(wire::Control::User(cmd))?;
        Ok(())
    }
}

impl<G: Signer + EcSign + 'static> radicle::node::Handle for Handle<G> {
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
        // Send a shutdown request to our own control socket. This is the only way to kill the
        // control thread gracefully. Since the control thread may have called this function,
        // the control socket may already be disconnected. Ignore errors.
        UnixStream::connect(self.home.socket())
            .and_then(|mut sock| sock.write_all(b"shutdown"))
            .ok();

        self.controller.shutdown().map_err(|_| Error::NotConnected)
    }
}
