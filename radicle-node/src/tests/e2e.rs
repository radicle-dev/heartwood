use std::path::Path;
use std::{
    collections::{BTreeMap, BTreeSet},
    iter, net, thread,
    time::Duration,
};

use radicle::crypto::test::signer::MockSigner;
use radicle::git::refname;
use radicle::identity::Id;
use radicle::node::Handle;
use radicle::profile::Home;
use radicle::storage::{ReadStorage, WriteStorage};
use radicle::test::fixtures;
use radicle::Storage;
use radicle::{assert_matches, rad};

use crate::address;
use crate::node::NodeId;
use crate::service::{routing, FetchLookup, FetchResult};
use crate::storage::git::transport;
use crate::test::logger;
use crate::wire::Wire;
use crate::{client, client::Runtime, service};

/// Represents a running node.
struct Node {
    id: NodeId,
    addr: net::SocketAddr,
    handle: client::handle::Handle<Wire<routing::Table, address::Book, Storage, MockSigner>>,
    signer: MockSigner,
    storage: Storage,
    #[allow(dead_code)]
    thread: thread::JoinHandle<Result<(), client::Error>>,
}

impl Node {
    /// Spawn a node in its own thread.
    fn spawn(base: &Path, config: service::Config) -> Self {
        let home = base.join(
            iter::repeat_with(fastrand::alphanumeric)
                .take(8)
                .collect::<String>(),
        );
        let paths = Home::init(home).unwrap();
        let signer = MockSigner::default();
        let listen = vec![([0, 0, 0, 0], 0).into()];
        let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
        let storage = Storage::open(paths.storage()).unwrap();
        let rt = Runtime::with(paths, config, listen, proxy, signer.clone()).unwrap();
        let addr = *rt.local_addrs.first().unwrap();
        let id = rt.id;
        let handle = rt.handle.clone();
        let thread = thread::Builder::new()
            .name(id.to_string())
            .spawn(|| rt.run())
            .unwrap();

        Self {
            id,
            addr,
            handle,
            signer,
            storage,
            thread,
        }
    }

    /// Connect this node to another node, and wait for the connection to be established both ways.
    fn connect(&mut self, remote: &Node) {
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
                break;
            }
            thread::sleep(Duration::from_millis(100));
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

        log::debug!(target: "test", "Initialized project {id} for node {}", self.id);

        id
    }
}

/// Checks whether the nodes have converged in their routing tables.
#[track_caller]
fn check<'a>(nodes: impl IntoIterator<Item = &'a Node>) -> BTreeSet<(Id, NodeId)> {
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

    let mut alice = Node::spawn(tmp.path(), service::Config::default());
    let mut bob = Node::spawn(tmp.path(), service::Config::default());

    alice.project("alice");
    bob.project("bob");
    alice.connect(&bob);

    let routes = check([&alice, &bob]);
    assert_eq!(routes.len(), 2);
}

#[test]
//
//     alice -- bob -- eve
//
fn test_inventory_sync_bridge() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();

    let mut alice = Node::spawn(tmp.path(), service::Config::default());
    let mut bob = Node::spawn(tmp.path(), service::Config::default());
    let mut eve = Node::spawn(tmp.path(), service::Config::default());

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");

    alice.connect(&bob);
    bob.connect(&eve);

    let routes = check([&alice, &bob, &eve]);
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

    let mut alice = Node::spawn(tmp.path(), service::Config::default());
    let mut bob = Node::spawn(tmp.path(), service::Config::default());
    let mut eve = Node::spawn(tmp.path(), service::Config::default());
    let mut carol = Node::spawn(tmp.path(), service::Config::default());

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");
    carol.project("carol");

    alice.connect(&bob);
    bob.connect(&eve);
    eve.connect(&carol);
    carol.connect(&alice);

    let routes = check([&alice, &bob, &eve, &carol]);
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

    let mut alice = Node::spawn(tmp.path(), service::Config::default());
    let mut bob = Node::spawn(tmp.path(), service::Config::default());
    let mut eve = Node::spawn(tmp.path(), service::Config::default());
    let mut carol = Node::spawn(tmp.path(), service::Config::default());
    let mut dave = Node::spawn(tmp.path(), service::Config::default());

    alice.project("alice");
    bob.project("bob");
    eve.project("eve");
    carol.project("carol");
    dave.project("dave");

    bob.connect(&alice);
    eve.connect(&alice);
    carol.connect(&alice);
    dave.connect(&alice);

    let routes = check([&alice, &bob, &eve, &carol, &dave]);
    assert_eq!(routes.len(), 5);
}

#[test]
#[ignore]
fn test_replication() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Node::spawn(tmp.path(), service::Config::default());
    let mut bob = Node::spawn(tmp.path(), service::Config::default());
    let acme = bob.project("acme");

    alice.connect(&bob);
    check([&alice, &bob]);

    let inventory = alice.handle.inventory().unwrap();
    assert!(inventory.try_iter().next().is_none());

    let tracked = alice.handle.track_repo(acme).unwrap();
    assert!(tracked);

    let (seeds, results) = match alice.handle.fetch(acme).unwrap() {
        FetchLookup::Found { seeds, results } => (seeds, results),
        other => panic!("Fetch lookup failed, got {:?}", other),
    };
    assert_eq!(seeds, nonempty::NonEmpty::new(bob.id));

    let (from, updated) = match results.recv_timeout(Duration::from_secs(6)).unwrap() {
        FetchResult::Fetched { from, updated } => (from, updated),
        FetchResult::Error { from, error } => {
            panic!("Fetch failed from {from}: {error}");
        }
    };
    assert_eq!(from, bob.id);
    assert_eq!(updated, vec![]);

    let inventory = alice.handle.inventory().unwrap();
    assert_eq!(inventory.try_iter().next(), Some(acme));
    assert_eq!(
        alice
            .storage
            .repository(acme)
            .unwrap()
            .remotes()
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<_>>(),
        bob.storage
            .repository(acme)
            .unwrap()
            .remotes()
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<_>>(),
    );
    assert_matches!(alice.storage.repository(acme).unwrap().verify(), Ok(()));
}
