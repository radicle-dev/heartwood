use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    iter, net, thread,
    time::Duration,
};

use radicle::crypto::dh;
use radicle::crypto::ssh::{keystore::MemorySigner, Keystore};
use radicle::crypto::test::signer::MockSigner;
use radicle::crypto::{KeyPair, Seed, Signer};
use radicle::git::refname;
use radicle::identity::Id;
use radicle::node::Handle as _;
use radicle::profile::Home;
use radicle::profile::Profile;
use radicle::rad;
use radicle::storage::ReadStorage;
use radicle::test::fixtures;
use radicle::Storage;

use crate::node::NodeId;
use crate::storage::git::transport;
use crate::{runtime, runtime::Handle, service, Runtime};

pub use service::Config;

/// Test environment.
pub struct Environment {
    tempdir: tempfile::TempDir,
    users: usize,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            tempdir: tempfile::tempdir().unwrap(),
            users: 0,
        }
    }
}

impl Environment {
    /// Create a new test environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the temp directory path.
    pub fn tmp(&self) -> PathBuf {
        self.tempdir.path().join("misc")
    }

    /// Create a new node in this environment. This should be used when a running node
    /// is required. Use [`Environment::profile`] otherwise.
    pub fn node(&mut self, name: &str) -> Node<MemorySigner> {
        let profile = self.profile(name);
        let signer = MemorySigner::load(&profile.keystore, "radicle".to_owned().into()).unwrap();

        Node {
            id: *profile.id(),
            home: profile.home,
            signer,
            storage: profile.storage,
        }
    }

    /// Create a new profile in this environment.
    /// This should be used when a running node is not required.
    pub fn profile(&mut self, name: &str) -> Profile {
        let home = Home::new(self.tmp().join("home").join(name))
            .init()
            .unwrap();
        let storage = Storage::open(home.storage()).unwrap();
        let keystore = Keystore::new(&home.keys());
        let keypair = KeyPair::from_seed(Seed::from([!(self.users as u8); 32]));

        transport::local::register(storage.clone());
        keystore
            .store(keypair.clone(), "radicle", "radicle".to_owned())
            .unwrap();

        // Ensures that each user has a unique but deterministic public key.
        self.users += 1;

        Profile {
            home,
            storage,
            keystore,
            public_key: keypair.pk.into(),
        }
    }
}

/// A node that can be run.
pub struct Node<G> {
    pub id: NodeId,
    pub home: Home,
    pub signer: G,
    pub storage: Storage,
}

/// Handle to a running node.
pub struct NodeHandle<G: Signer + cyphernet::Ecdh + 'static> {
    pub id: NodeId,
    pub storage: Storage,
    pub signer: G,
    pub home: Home,
    pub addr: net::SocketAddr,
    pub thread: ManuallyDrop<thread::JoinHandle<Result<(), runtime::Error>>>,
    pub handle: ManuallyDrop<Handle<G>>,
}

impl<G: Signer + cyphernet::Ecdh + 'static> Drop for NodeHandle<G> {
    fn drop(&mut self) {
        log::debug!(target: "test", "Node {} shutting down..", self.id);

        unsafe { ManuallyDrop::take(&mut self.handle) }
            .shutdown()
            .unwrap();
        unsafe { ManuallyDrop::take(&mut self.thread) }
            .join()
            .unwrap()
            .unwrap();
    }
}

impl<G: Signer + cyphernet::Ecdh> NodeHandle<G> {
    /// Connect this node to another node, and wait for the connection to be established both ways.
    pub fn connect(&mut self, remote: &NodeHandle<G>) -> &mut Self {
        self.handle.connect(remote.id, remote.addr.into()).unwrap();

        loop {
            let local_sessions = self.handle.sessions().unwrap();
            let remote_sessions = remote.handle.sessions().unwrap();

            let local_sessions = local_sessions
                .negotiated()
                .map(|(id, _)| id)
                .collect::<BTreeSet<_>>();
            let remote_sessions = remote_sessions
                .negotiated()
                .map(|(id, _)| id)
                .collect::<BTreeSet<_>>();

            if local_sessions.contains(&remote.id) && remote_sessions.contains(&self.id) {
                log::debug!(target: "test", "Connection between {} and {} established", self.id, remote.id);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        self
    }

    /// Wait until this node's routing table matches the remotes.
    pub fn converge<'a>(
        &'a self,
        remotes: impl IntoIterator<Item = &'a NodeHandle<G>>,
    ) -> BTreeSet<(Id, NodeId)> {
        converge(iter::once(self).chain(remotes.into_iter()))
    }
}

impl Node<MockSigner> {
    /// Create a new node.
    pub fn init(base: &Path) -> Self {
        let home = base.join(
            iter::repeat_with(fastrand::alphanumeric)
                .take(8)
                .collect::<String>(),
        );
        let home = Home::new(home).init().unwrap();
        let signer = MockSigner::default();
        let storage = Storage::open(home.storage()).unwrap();

        Self {
            id: *signer.public_key(),
            home,
            signer,
            storage,
        }
    }
}

impl<G: cyphernet::Ecdh<Pk = dh::PublicKey> + Signer + Clone> Node<G> {
    /// Spawn a node in its own thread.
    pub fn spawn(self, config: service::Config) -> NodeHandle<G> {
        let listen = vec![([0, 0, 0, 0], 0).into()];
        let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
        let daemon = ([0, 0, 0, 0], fastrand::u16(1025..)).into();
        let rt = Runtime::init(
            self.home.clone(),
            config,
            listen,
            proxy,
            daemon,
            self.signer.clone(),
        )
        .unwrap();
        let addr = *rt.local_addrs.first().unwrap();
        let id = *self.signer.public_key();
        let handle = ManuallyDrop::new(rt.handle.clone());
        let thread = ManuallyDrop::new(
            thread::Builder::new()
                .name(id.to_string())
                .spawn(move || rt.run())
                .unwrap(),
        );

        NodeHandle {
            id,
            storage: self.storage,
            signer: self.signer,
            home: self.home,
            addr,
            handle,
            thread,
        }
    }

    /// Populate a storage instance with a project.
    pub fn project(&mut self, name: &str, description: &str) -> Id {
        transport::local::register(self.storage.clone());

        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = fixtures::gen::repository(tmp.path());
        let id = rad::init(
            &repo,
            name,
            description,
            refname!("master"),
            &self.signer,
            &self.storage,
        )
        .map(|(id, _, _)| id)
        .unwrap();

        log::debug!(
            target: "test",
            "Initialized project {id} for node {}", self.signer.public_key()
        );

        id
    }
}

/// Checks whether the nodes have converged in their routing tables.
#[track_caller]
pub fn converge<'a, G: Signer + cyphernet::Ecdh + 'static>(
    nodes: impl IntoIterator<Item = &'a NodeHandle<G>>,
) -> BTreeSet<(Id, NodeId)> {
    let nodes = nodes.into_iter().collect::<Vec<_>>();

    let mut all_routes = BTreeSet::<(Id, NodeId)>::new();
    let mut remaining = BTreeMap::from_iter(nodes.iter().map(|node| (node.id, node)));

    // First build the set of all routes.
    for node in &nodes {
        let inv = node.storage.inventory().unwrap();

        for rid in inv {
            all_routes.insert((rid, node.id));
        }
    }

    // Then, while there are nodes remaining to converge, check each node to see if
    // its routing table has all routes. If so, remove it from the remaining nodes.
    while !remaining.is_empty() {
        remaining.retain(|_, node| {
            let routing = node.handle.routing().unwrap();
            let routes = BTreeSet::from_iter(routing.try_iter());

            if routes == all_routes {
                log::debug!(target: "test", "Node {} has converged", node.id);
                return false;
            } else {
                log::debug!(target: "test", "Node {} has {:?}", node.id, routes);
            }
            true
        });
        thread::sleep(Duration::from_millis(100));
    }
    all_routes
}
