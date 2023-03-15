use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::identity::Id;
use crate::node::{FetchResult, Seeds};
use crate::runtime::HandleError;
use crate::service::NodeId;
use crate::service::{self, tracking};
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

    fn is_running(&self) -> bool {
        true
    }

    fn connect(&mut self, _node: NodeId, _addr: radicle::node::Address) -> Result<(), Self::Error> {
        unimplemented!();
    }

    fn seeds(&mut self, _id: Id) -> Result<Seeds, Self::Error> {
        unimplemented!();
    }

    fn fetch(&mut self, _id: Id, _from: NodeId) -> Result<FetchResult, Self::Error> {
        Ok(FetchResult::from(Ok::<Vec<RefUpdate>, Self::Error>(vec![])))
    }

    fn track_repo(&mut self, id: Id, _scope: tracking::Scope) -> Result<bool, Self::Error> {
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

    fn announce_inventory(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn sync_inventory(&mut self) -> Result<bool, Self::Error> {
        unimplemented!()
    }

    fn sessions(&self) -> Result<Self::Sessions, Self::Error> {
        unimplemented!();
    }

    fn shutdown(self) -> Result<(), Self::Error> {
        Ok(())
    }
}
