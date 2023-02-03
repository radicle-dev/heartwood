use std::mem::ManuallyDrop;
use std::path::Path;
use std::{
    collections::{BTreeMap, BTreeSet},
    iter, net, thread,
    time::Duration,
};

use radicle::crypto::test::signer::MockSigner;
use radicle::crypto::Signer;
use radicle::git::refname;
use radicle::identity::Id;
use radicle::node::FetchLookup;
use radicle::node::Handle as _;
use radicle::profile::Home;
use radicle::storage::{ReadRepository, ReadStorage, WriteStorage};
use radicle::test::fixtures;
use radicle::Storage;
use radicle::{assert_matches, rad};

use crate::node::NodeId;
use crate::storage::git::transport;
use crate::test::logger;
use crate::{runtime, runtime::Handle, service, Runtime};

/// A node that can be run.
struct Node {
    home: Home,
    signer: MockSigner,
    storage: Storage,
}

/// Handle to a running node.
struct NodeHandle {
    id: NodeId,
    storage: Storage,
    signer: MockSigner,
    addr: net::SocketAddr,
    thread: ManuallyDrop<thread::JoinHandle<Result<(), runtime::Error>>>,
    handle: ManuallyDrop<Handle<MockSigner>>,
}

impl Drop for NodeHandle {
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

impl NodeHandle {
    /// Connect this node to another node, and wait for the connection to be established both ways.
    fn connect(&mut self, remote: &NodeHandle) {
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
    }
}

impl Node {
    /// Create a new node.
    fn new(base: &Path) -> Self {
        let home = base.join(
            iter::repeat_with(fastrand::alphanumeric)
                .take(8)
                .collect::<String>(),
        );
        let home = Home::new(home).init().unwrap();
        let signer = MockSigner::default();
        let storage = Storage::open(home.storage()).unwrap();

        Self {
            home,
            signer,
            storage,
        }
    }

    /// Spawn a node in its own thread.
    fn spawn(self, config: service::Config) -> NodeHandle {
        let listen = vec![([0, 0, 0, 0], 0).into()];
        let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
        let daemon = ([0, 0, 0, 0], fastrand::u16(1025..)).into();
        let rt = Runtime::init(
            self.home,
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
            addr,
            handle,
            thread,
        }
    }

    /// Populate a storage instance with a project.
    fn project(&mut self, name: &str) -> Id {
        transport::local::register(self.storage.clone());

        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = fixtures::gen::repository(tmp.path());
        let description = iter::repeat_with(fastrand::alphabetic)
            .take(12)
            .collect::<String>();
        let id = rad::init(
            &repo,
            name,
            &description,
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
fn converge<'a>(nodes: impl IntoIterator<Item = &'a NodeHandle>) -> BTreeSet<(Id, NodeId)> {
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

#[test]
//
//     alice -- bob
//
fn test_inventory_sync_basic() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());

    alice.project("alice");
    bob.project("bob");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);

    let routes = converge([&alice, &bob]);
    assert_eq!(routes.len(), 2);
}

#[test]
//
//     alice -- bob -- eve
//
fn test_inventory_sync_bridge() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());
    let mut eve = Node::new(tmp.path());

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());
    let eve = eve.spawn(service::Config::default());

    alice.connect(&bob);
    bob.connect(&eve);

    let routes = converge([&alice, &bob, &eve]);
    assert_eq!(routes.len(), 3);
}

#[test]
//
//     alice -- bob
//       |       |
//     carol -- eve
//
fn test_inventory_sync_ring() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());
    let mut eve = Node::new(tmp.path());
    let mut carol = Node::new(tmp.path());

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");
    carol.project("carol");

    let mut alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());
    let mut eve = eve.spawn(service::Config::default());
    let mut carol = carol.spawn(service::Config::default());

    alice.connect(&bob);
    bob.connect(&eve);
    eve.connect(&carol);
    carol.connect(&alice);

    let routes = converge([&alice, &bob, &eve, &carol]);
    assert_eq!(routes.len(), 4);
}

#[test]
//
//             dave
//              |
//     eve -- alice -- bob
//              |
//            carol
//
fn test_inventory_sync_star() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());
    let mut eve = Node::new(tmp.path());
    let mut carol = Node::new(tmp.path());
    let mut dave = Node::new(tmp.path());

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");
    carol.project("carol");
    dave.project("dave");

    let alice = alice.spawn(service::Config::default());
    let mut bob = bob.spawn(service::Config::default());
    let mut eve = eve.spawn(service::Config::default());
    let mut carol = carol.spawn(service::Config::default());
    let mut dave = dave.spawn(service::Config::default());

    bob.connect(&alice);
    eve.connect(&alice);
    carol.connect(&alice);
    dave.connect(&alice);

    let routes = converge([&alice, &bob, &eve, &carol, &dave]);
    assert_eq!(routes.len(), 5);
}

#[test]
fn test_replication() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());
    let acme = bob.project("acme");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    let inventory = alice.handle.inventory().unwrap();
    assert!(inventory.try_iter().next().is_none());

    let tracked = alice.handle.track_repo(acme).unwrap();
    assert!(tracked);

    let (seeds, results) = match alice.handle.fetch(acme).unwrap() {
        FetchLookup::Found { seeds, results } => (seeds, results),
        other => panic!("Fetch lookup failed, got {other:?}"),
    };
    assert_eq!(seeds, nonempty::NonEmpty::new(bob.id));

    let result = results.recv_timeout(Duration::from_secs(6)).unwrap();
    let updated = match result.result {
        Ok(updated) => updated,
        Err(err) => {
            panic!("Fetch failed from {}: {err}", result.remote);
        }
    };
    assert_eq!(result.remote, bob.id);
    assert_eq!(updated, vec![]);

    log::debug!(target: "test", "Fetch complete with {}", result.remote);

    let inventory = alice.handle.inventory().unwrap();
    let alice_repo = alice.storage.repository(acme).unwrap();
    let bob_repo = bob.storage.repository(acme).unwrap();

    let alice_refs = alice_repo
        .references()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let bob_refs = bob_repo
        .references()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(inventory.try_iter().next(), Some(acme));
    assert_eq!(alice_refs, bob_refs);
    assert_matches!(alice.storage.repository(acme).unwrap().verify(), Ok(()));
}

#[test]
fn test_clone() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());
    let acme = bob.project("acme");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.track_repo(acme).unwrap();
    let lookup = alice.handle.fetch(acme).unwrap();

    match lookup {
        // Drain the channel.
        FetchLookup::Found { seeds, results } => for _ in results.iter().take(seeds.len()) {},
        other => panic!("Unexpected fetch lookup: {other:?}"),
    }
    rad::fork(acme, &alice.signer, &alice.storage).unwrap();

    let working = rad::checkout(
        acme,
        alice.signer.public_key(),
        tmp.path().join("clone"),
        &alice.storage,
    )
    .unwrap();

    // Makes test finish faster.
    drop(alice);

    let head = working.head().unwrap();
    let oid = head.target().unwrap();

    let (_, canonical) = bob
        .storage
        .repository(acme)
        .unwrap()
        .canonical_head()
        .unwrap();

    assert_eq!(oid, *canonical);
}

#[test]
fn test_fetch_up_to_date() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let alice = Node::new(tmp.path());
    let mut bob = Node::new(tmp.path());
    let acme = bob.project("acme");

    let mut alice = alice.spawn(service::Config::default());
    let bob = bob.spawn(service::Config::default());

    alice.connect(&bob);
    converge([&alice, &bob]);

    transport::local::register(alice.storage.clone());

    let _ = alice.handle.track_repo(acme).unwrap();

    match alice.handle.fetch(acme).unwrap() {
        FetchLookup::Found { seeds, results } => for _ in results.iter().take(seeds.len()) {},
        other => panic!("Unexpected fetch lookup: {other:?}"),
    }

    // Fetch again! This time, everything's up to date.
    match alice.handle.fetch(acme).unwrap() {
        FetchLookup::Found { seeds, results } => for _ in results.iter().take(seeds.len()) {},
        other => panic!("Unexpected fetch lookup: {other:?}"),
    }
}
