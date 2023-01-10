use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    net, thread,
};

use radicle::crypto::ssh::keystore::MemorySigner;
use radicle::git::refname;
use radicle::identity::Id;
use radicle::node::Handle;
use radicle::rad;
use radicle::test::fixtures;
use radicle::Profile;
use radicle::Storage;

use crate::address;
use crate::node::NodeId;
use crate::service::routing;
use crate::storage::git::transport;
use crate::test::logger;
use crate::wire::Transport;
use crate::{client, client::Runtime, service};

type TestHandle = (
    client::handle::Handle<Transport<routing::Table, address::Book, Storage, MemorySigner>>,
    thread::JoinHandle<Result<(), client::Error>>,
);

/// Populate a storage instance with a project.
fn populate(storage: &Storage, signer: &MemorySigner) {
    transport::local::register(storage.clone());

    let tmp = tempfile::tempdir().unwrap();
    let (repo, _) = fixtures::repository(tmp.path().join("acme"));

    rad::init(
        &repo,
        "acme",
        "Acme's Repo",
        refname!("master"),
        signer,
        storage,
    )
    .unwrap();
}

/// Create a node runtime.
fn runtime(home: &Path, config: service::Config) -> Runtime<MemorySigner> {
    let profile = Profile::init(home, "pasphrase".to_owned()).unwrap();
    let signer = MemorySigner::load(&profile.keystore, "pasphrase".to_owned().into()).unwrap();
    let listen = vec![([0, 0, 0, 0], 0).into()];
    let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);

    populate(&profile.storage, &signer);

    Runtime::with(profile, config, listen, proxy, signer).unwrap()
}

/// Create a network of nodes connected to each other.
fn network(
    configs: impl IntoIterator<Item = (service::Config, PathBuf)>,
) -> BTreeMap<(NodeId, net::SocketAddr), TestHandle> {
    let mut runtimes = BTreeMap::new();
    for (config, home) in configs.into_iter() {
        let rt = runtime(home.as_ref(), config);
        let id = rt.id;
        let addr = *rt.local_addrs.first().unwrap();
        let handle = rt.handle.clone();
        let join = thread::spawn(|| rt.run());

        runtimes.insert((id, addr), (handle, join));
    }

    let mut connect = Vec::new();
    for (i, (from, _)) in runtimes.iter().enumerate() {
        let peers = runtimes
            .iter()
            .skip(i + 1)
            .map(|(p, _)| *p)
            .collect::<Vec<(NodeId, net::SocketAddr)>>();
        for to in peers {
            connect.push((*from, to));
        }
    }

    println!("{:#?}", runtimes.keys());

    for (from, (to_id, to_addr)) in connect {
        let (handle, _) = runtimes.get_mut(&from).unwrap();
        println!("{} => {}", from.0, to_id);
        handle.connect(to_id, to_addr.into()).unwrap();
    }
    runtimes
}

/// Checks whether the nodes have converged in their routing tables.
#[track_caller]
fn check(
    nodes: impl IntoIterator<Item = ((NodeId, net::SocketAddr), TestHandle)>,
) -> BTreeSet<(Id, NodeId)> {
    let mut by_node = BTreeMap::<NodeId, BTreeSet<(Id, NodeId)>>::new();
    let mut all = BTreeSet::<(Id, NodeId)>::new();

    for ((id, _), (handle, _)) in nodes {
        let routing = handle.routing().unwrap();

        for (rid, node) in routing.try_iter() {
            all.insert((rid, node));
            by_node
                .entry(id)
                .or_insert_with(BTreeSet::new)
                .insert((rid, node));
        }
    }

    for (node, routes) in by_node {
        assert_eq!(routes, all, "{node} failed to converge");
    }
    all
}

#[test]
fn test_e2e() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    let nodes = network(vec![
        (service::Config::default(), base.join("alice")),
        (service::Config::default(), base.join("bob")),
        (service::Config::default(), base.join("eve")),
    ]);
    // TODO: Find a better way to wait for synchronization, eg. using events, or using a loop.
    thread::sleep(std::time::Duration::from_secs(3));

    let routes = check(nodes);
    assert_eq!(routes.len(), 3);
}
