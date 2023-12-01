use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::{io, time};

use radicle::git;
use radicle::storage::refs::RefsAt;

use crate::identity::Id;
use crate::node::{Alias, Config, ConnectOptions, ConnectResult, Event, FetchResult, Seeds};
use crate::runtime::HandleError;
use crate::service::policy;
use crate::service::NodeId;

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<Id>>>,
    pub tracking_repos: Arc<Mutex<HashSet<Id>>>,
    pub tracking_nodes: Arc<Mutex<HashSet<NodeId>>>,
}

impl radicle::node::Handle for Handle {
    type Error = HandleError;
    type Sessions = Vec<radicle::node::Session>;

    fn nid(&self) -> Result<NodeId, Self::Error> {
        Ok(NodeId::from_str("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap())
    }

    fn is_running(&self) -> bool {
        true
    }

    fn config(&self) -> Result<Config, Self::Error> {
        Ok(Config::new(Alias::new("acme")))
    }

    fn connect(
        &mut self,
        _node: NodeId,
        _addr: radicle::node::Address,
        _opts: ConnectOptions,
    ) -> Result<ConnectResult, Self::Error> {
        unimplemented!();
    }

    fn seeds(&mut self, _id: Id) -> Result<Seeds, Self::Error> {
        unimplemented!();
    }

    fn fetch(
        &mut self,
        _id: Id,
        _from: NodeId,
        _timeout: time::Duration,
    ) -> Result<FetchResult, Self::Error> {
        Ok(FetchResult::Success {
            updated: vec![],
            namespaces: HashSet::new(),
        })
    }

    fn seed(&mut self, id: Id, _scope: policy::Scope) -> Result<bool, Self::Error> {
        Ok(self.tracking_repos.lock().unwrap().insert(id))
    }

    fn unseed(&mut self, id: Id) -> Result<bool, Self::Error> {
        Ok(self.tracking_repos.lock().unwrap().remove(&id))
    }

    fn follow(&mut self, id: NodeId, _alias: Option<Alias>) -> Result<bool, Self::Error> {
        Ok(self.tracking_nodes.lock().unwrap().insert(id))
    }

    fn subscribe(
        &self,
        _timeout: time::Duration,
    ) -> Result<Box<dyn Iterator<Item = Result<Event, io::Error>>>, Self::Error> {
        Ok(Box::new(std::iter::empty()))
    }

    fn unfollow(&mut self, id: NodeId) -> Result<bool, Self::Error> {
        Ok(self.tracking_nodes.lock().unwrap().remove(&id))
    }

    fn announce_refs(&mut self, id: Id) -> Result<RefsAt, Self::Error> {
        self.updates.lock().unwrap().push(id);

        Ok(RefsAt {
            remote: self.nid()?,
            at: git::raw::Oid::zero().into(),
        })
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
