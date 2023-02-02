use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crossbeam_channel as chan;

use crate::identity::Id;
use crate::node::FetchResult;
use crate::runtime::HandleError;
use crate::service;
use crate::service::NodeId;
use crate::storage::RefUpdate;

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<Id>>>,
    pub tracking_repos: HashSet<Id>,
    pub tracking_nodes: HashSet<NodeId>,
}

impl radicle::node::Handle for Handle {
    type Error = HandleError;
    type Sessions = service::Sessions;
    type FetchResult = FetchResult;

    fn is_running(&self) -> bool {
        true
    }

    fn connect(&mut self, _node: NodeId, _addr: radicle::node::Address) -> Result<(), Self::Error> {
        unimplemented!();
    }

    fn seeds(&mut self, _id: Id) -> Result<Vec<NodeId>, Self::Error> {
        unimplemented!();
    }

    fn fetch(&mut self, _id: Id, _from: NodeId) -> Result<FetchResult, Self::Error> {
        Ok(FetchResult::from(Ok::<Vec<RefUpdate>, Self::Error>(vec![])))
    }

    fn track_repo(&mut self, id: Id) -> Result<bool, Self::Error> {
        Ok(self.tracking_repos.insert(id))
    }

    fn untrack_repo(&mut self, id: Id) -> Result<bool, Self::Error> {
        Ok(self.tracking_repos.remove(&id))
    }

    fn track_node(&mut self, id: NodeId, _alias: Option<String>) -> Result<bool, Self::Error> {
        Ok(self.tracking_nodes.insert(id))
    }

    fn untrack_node(&mut self, id: NodeId) -> Result<bool, Self::Error> {
        Ok(self.tracking_nodes.remove(&id))
    }

    fn announce_refs(&mut self, id: Id) -> Result<(), Self::Error> {
        self.updates.lock().unwrap().push(id);

        Ok(())
    }

    fn routing(&self) -> Result<chan::Receiver<(Id, service::NodeId)>, Self::Error> {
        unimplemented!();
    }

    fn sessions(&self) -> Result<Self::Sessions, Self::Error> {
        unimplemented!();
    }

    fn inventory(&self) -> Result<chan::Receiver<Id>, Self::Error> {
        unimplemented!();
    }

    fn shutdown(self) -> Result<(), Self::Error> {
        Ok(())
    }
}
