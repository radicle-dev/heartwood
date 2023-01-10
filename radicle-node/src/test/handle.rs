use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crossbeam_channel as chan;

use crate::client::handle::Error;
use crate::identity::Id;
use crate::service;
use crate::service::FetchLookup;
use crate::service::NodeId;

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<Id>>>,
    pub tracking_repos: HashSet<Id>,
    pub tracking_nodes: HashSet<NodeId>,
}

impl radicle::node::Handle for Handle {
    type Error = Error;
    type Session = service::Session;
    type FetchLookup = FetchLookup;

    fn connect(&mut self, _node: NodeId, _addr: radicle::node::Address) -> Result<(), Error> {
        unimplemented!();
    }

    fn fetch(&mut self, _id: Id) -> Result<FetchLookup, Error> {
        Ok(FetchLookup::NotFound)
    }

    fn track_repo(&mut self, id: Id) -> Result<bool, Error> {
        Ok(self.tracking_repos.insert(id))
    }

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Error> {
        Ok(self.tracking_repos.remove(&id))
    }

    fn track_node(&mut self, id: NodeId, _alias: Option<String>) -> Result<bool, Error> {
        Ok(self.tracking_nodes.insert(id))
    }

    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Error> {
        Ok(self.tracking_nodes.remove(&id))
    }

    fn announce_refs(&mut self, id: Id) -> Result<(), Error> {
        self.updates.lock().unwrap().push(id);

        Ok(())
    }

    fn routing(&self) -> Result<chan::Receiver<(Id, service::NodeId)>, Error> {
        unimplemented!();
    }

    fn sessions(&self) -> Result<chan::Receiver<(service::NodeId, service::Session)>, Error> {
        unimplemented!();
    }

    fn inventory(&self) -> Result<chan::Receiver<Id>, Error> {
        unimplemented!();
    }

    fn shutdown(self) -> Result<(), Error> {
        Ok(())
    }
}
