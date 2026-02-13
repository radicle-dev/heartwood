use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time;

use radicle::crypto::PublicKey;
use radicle::git::Oid;
use radicle::storage::refs::RefsAt;

use crate::identity::RepoId;
use crate::node::{Alias, Config, ConnectOptions, ConnectResult, Event, FetchResult, Seeds};
use crate::runtime::HandleError;
use radicle::node::policy;
use radicle::node::NodeId;

#[derive(Default, Clone)]
pub struct Handle {
    pub updates: Arc<Mutex<Vec<(RepoId, PublicKey)>>>,
    pub seeding: Arc<Mutex<HashSet<RepoId>>>,
    pub following: Arc<Mutex<HashSet<NodeId>>>,
    pub blocked: Arc<Mutex<HashSet<NodeId>>>,
}

impl radicle::node::Handle for Handle {
    type Error = HandleError;
    type Sessions = Vec<radicle::node::Session>;
    type Events = Vec<Self::Event>;
    type Event = Result<Event, Self::Error>;

    fn nid(&self) -> Result<NodeId, Self::Error> {
        Ok(NodeId::from_str("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap())
    }

    fn is_running(&self) -> bool {
        true
    }

    fn listen_addrs(&self) -> Result<Vec<std::net::SocketAddr>, Self::Error> {
        Ok(vec![])
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

    fn disconnect(&mut self, _node: NodeId) -> Result<(), Self::Error> {
        unimplemented!();
    }

    fn seeds_for(
        &mut self,
        _id: RepoId,
        _namespaces: impl IntoIterator<Item = PublicKey>,
    ) -> Result<Seeds, Self::Error> {
        unimplemented!();
    }

    fn fetch(
        &mut self,
        _id: RepoId,
        _from: NodeId,
        _timeout: time::Duration,
    ) -> Result<FetchResult, Self::Error> {
        Ok(FetchResult::Success {
            updated: vec![],
            namespaces: HashSet::new(),
            clone: false,
        })
    }

    fn seed(&mut self, id: RepoId, _scope: policy::Scope) -> Result<bool, Self::Error> {
        Ok(self.seeding.lock().unwrap().insert(id))
    }

    fn unseed(&mut self, id: RepoId) -> Result<bool, Self::Error> {
        Ok(self.seeding.lock().unwrap().remove(&id))
    }

    fn follow(&mut self, id: NodeId, _alias: Option<Alias>) -> Result<bool, Self::Error> {
        self.blocked.lock().unwrap().remove(&id);
        Ok(self.following.lock().unwrap().insert(id))
    }

    fn subscribe(&self, _timeout: time::Duration) -> Result<Self::Events, Self::Error> {
        Ok(vec![])
    }

    fn unfollow(&mut self, id: NodeId) -> Result<bool, Self::Error> {
        let f = self.following.lock().unwrap().remove(&id);
        let b = self.blocked.lock().unwrap().remove(&id);
        Ok(f || b)
    }

    fn block(&mut self, id: NodeId) -> Result<bool, Self::Error> {
        self.following.lock().unwrap().remove(&id);
        Ok(self.blocked.lock().unwrap().insert(id))
    }

    fn announce_refs_for(
        &mut self,
        id: RepoId,
        namespaces: impl IntoIterator<Item = PublicKey>,
    ) -> Result<RefsAt, Self::Error> {
        self.updates
            .lock()
            .unwrap()
            .extend(namespaces.into_iter().map(|ns| (id, ns)));

        Ok(RefsAt {
            remote: self.nid()?,
            at: Oid::sha1_zero(),
        })
    }

    fn announce_inventory(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn add_inventory(&mut self, _rid: RepoId) -> Result<bool, Self::Error> {
        unimplemented!()
    }

    fn sessions(&self) -> Result<Self::Sessions, Self::Error> {
        unimplemented!();
    }

    fn session(&self, _node: NodeId) -> Result<Option<radicle::node::Session>, Self::Error> {
        unimplemented!()
    }

    fn shutdown(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn debug(&self) -> Result<serde_json::Value, Self::Error> {
        Ok(serde_json::Value::Null)
    }
}
