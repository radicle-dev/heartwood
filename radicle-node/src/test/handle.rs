use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crossbeam_channel as chan;

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
    type Routing = Vec<(Id, NodeId)>;
    type TrackedRepos = Vec<tracking::Repo>;
    type TrackedNodes = Vec<tracking::Node>;

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

    fn tracked_repos(&self) -> Result<Self::TrackedRepos, Self::Error> {
        Ok(self
            .tracking_repos
            .iter()
            .copied()
            .map(|id| tracking::Repo {
                id,
                scope: tracking::Scope::All,
                policy: tracking::Policy::Track,
            })
            .collect())
    }

    fn tracked_nodes(&self) -> Result<Self::TrackedNodes, Self::Error> {
        Ok(self
            .tracking_nodes
            .iter()
            .copied()
            .map(|id| tracking::Node {
                id,
                alias: None,
                policy: tracking::Policy::Track,
            })
            .collect())
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

    fn routing(&self) -> Result<Self::Routing, Self::Error> {
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
