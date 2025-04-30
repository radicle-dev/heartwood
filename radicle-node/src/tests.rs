mod e2e;

use std::collections::BTreeSet;
use std::default::*;
use std::env;
use std::io;
use std::sync::Arc;
use std::time;

use crossbeam_channel as chan;
use netservices::Direction as Link;
use once_cell::sync::Lazy;
use radicle::identity::Visibility;
use radicle::node::address::Store as _;
use radicle::node::device::Device;
use radicle::node::refs::Store as _;
use radicle::node::routing::Store as _;
use radicle::node::{ConnectOptions, DEFAULT_TIMEOUT};
use radicle::storage::refs::RefsAt;
use radicle::storage::RefUpdate;
use radicle::test::arbitrary::gen;
use radicle::test::storage::MockRepository;

use crate::collections::{RandomMap, RandomSet};
use crate::identity::RepoId;
use crate::node;
use crate::node::config::*;
use crate::prelude::*;
use crate::prelude::{LocalDuration, Timestamp};
use crate::service::filter::Filter;
use crate::service::io::Io;
use crate::service::message::*;
use crate::service::ServiceState as _;
use crate::service::*;
use crate::storage::git::transport::{local, remote};
use crate::storage::git::Storage;
use crate::storage::refs::SIGREFS_BRANCH;
use crate::storage::ReadStorage;
use crate::test::arbitrary;
use crate::test::assert_matches;
use crate::test::fixtures;
#[allow(unused)]
use crate::test::logger;
use crate::test::peer;
use crate::test::peer::Peer;
use crate::test::simulator;
use crate::test::simulator::{Peer as _, Simulation};

use crate::test::storage::MockStorage;
use crate::wire::Decode;
use crate::wire::Encode;
use crate::worker;
use crate::worker::fetch;
use crate::LocalTime;
use crate::{git, identity, rad, runtime, service, test};

/// Default number of tests to run when testing things with high variance.
pub const DEFAULT_TEST_CASES: usize = 10;
/// Test cases to run when testing things with high variance.
pub static TEST_CASES: Lazy<usize> = Lazy::new(|| {
    env::var("RAD_TEST_CASES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_TEST_CASES)
});

// NOTE
//
// If you wish to see the logs for a running test, simply add the following line to your test:
//
//      logger::init(log::Level::Debug);
//
// You may then run the test with eg. `cargo test -- --nocapture` to always show output.

#[test]
fn test_inventory_decode() {
    let inventory: Vec<RepoId> = arbitrary::gen(300);
    let timestamp: Timestamp = LocalTime::now().into();

    let mut buf = Vec::new();
    inventory.as_slice().encode(&mut buf).unwrap();
    timestamp.encode(&mut buf).unwrap();

    let m = InventoryAnnouncement::decode(&mut buf.as_slice()).expect("message decodes");
    assert_eq!(inventory.as_slice(), m.inventory.as_slice());
    assert_eq!(timestamp, m.timestamp);
}

#[test]
fn test_ping_response() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8]);
    let bob = Peer::new("bob", [9, 9, 9, 9]);
    let eve = Peer::new("eve", [7, 7, 7, 7]);

    alice.connect_to(&bob);
    alice.receive(
        bob.id(),
        Message::Ping(Ping {
            ponglen: Ping::MAX_PONG_ZEROES,
            zeroes: ZeroBytes::new(42),
        }),
    );
    assert_matches!(
        alice.messages(bob.id()).next(),
        Some(Message::Pong { zeroes }) if zeroes.len() == Ping::MAX_PONG_ZEROES as usize,
        "respond with correctly formatted pong",
    );

    alice.connect_to(&eve);
    alice.receive(
        eve.id(),
        Message::Ping(Ping {
            ponglen: Ping::MAX_PONG_ZEROES + 1,
            zeroes: ZeroBytes::new(42),
        }),
    );
    assert_matches!(
        alice.messages(eve.id()).next(),
        None,
        "ignore unsupported ping message",
    );
}

#[test]
fn test_disconnecting_unresponsive_peer() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8]);
    let bob = Peer::new("bob", [9, 9, 9, 9]);

    alice.connect_to(&bob);
    assert_eq!(1, alice.sessions().connected().count(), "bob connects");
    alice.elapse(STALE_CONNECTION_TIMEOUT + LocalDuration::from_secs(1));
    alice
        .outbox()
        .find(|m| matches!(m, &Io::Disconnect(addr, _) if addr == bob.id()))
        .expect("disconnect an unresponsive bob");
}

#[test]
fn test_redundant_connect() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8]);
    let bob = Peer::new("bob", [9, 9, 9, 9]);
    let opts = ConnectOptions::default();

    alice.command(Command::Connect(bob.id(), bob.address(), opts.clone()));
    alice.command(Command::Connect(bob.id(), bob.address(), opts.clone()));
    alice.command(Command::Connect(bob.id(), bob.address(), opts));

    // Only one connection attempt is made.
    assert_matches!(
        alice.outbox().filter(|o| matches!(o, Io::Connect { .. })).collect::<Vec<_>>().as_slice(),
        [Io::Connect(id, addr)]
        if *id == bob.id() && *addr == bob.addr()
    );
}

#[test]
fn test_connection_kept_alive() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8]);
    let mut bob = Peer::new("bob", [9, 9, 9, 9]);

    let mut sim = Simulation::new(
        LocalTime::now(),
        alice.rng.clone(),
        simulator::Options::default(),
    )
    .initialize([&mut alice, &mut bob]);

    alice.command(service::Command::Connect(
        bob.id(),
        bob.address(),
        ConnectOptions::default(),
    ));
    sim.run_while([&mut alice, &mut bob], |s| !s.is_settled());
    assert_eq!(1, alice.sessions().connected().count(), "bob connects");

    let mut elapsed: LocalDuration = LocalDuration::from_secs(0);
    let step: LocalDuration = STALE_CONNECTION_TIMEOUT / 10;
    while elapsed < STALE_CONNECTION_TIMEOUT + step {
        alice.elapse(step);
        bob.elapse(step);
        sim.run_while([&mut alice, &mut bob], |s| !s.is_settled());

        elapsed = elapsed + step;
    }

    assert_eq!(1, alice.sessions().len(), "alice remains connected to Bob");
    assert_eq!(1, bob.sessions().len(), "bob remains connected to Alice");
}

#[test]
fn test_outbound_connection() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8]);
    let bob = Peer::new("bob", [9, 9, 9, 9]);
    let eve = Peer::new("eve", [7, 7, 7, 7]);

    alice.connect_to(&bob);
    alice.connect_to(&eve);

    let peers = alice
        .service
        .sessions()
        .connected()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    assert!(peers.contains(&eve.id()));
    assert!(peers.contains(&bob.id()));
}

#[test]
fn test_inbound_connection() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8]);
    let bob = Peer::new("bob", [9, 9, 9, 9]);
    let eve = Peer::new("eve", [7, 7, 7, 7]);

    alice.connect_from(&bob);
    alice.connect_from(&eve);

    let peers = alice
        .service
        .sessions()
        .connected()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    assert!(peers.contains(&eve.id()));
    assert!(peers.contains(&bob.id()));
}

#[test]
fn test_persistent_peer_connect() {
    use std::collections::HashSet;

    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let connect = HashSet::<ConnectAddress>::from_iter([
        (bob.id(), bob.address()).into(),
        (eve.id(), eve.address()).into(),
    ]);
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        MockStorage::empty(),
        peer::Config {
            config: Config {
                connect,
                ..Config::new(node::Alias::new("alice"))
            },
            ..peer::Config::default()
        },
    )
    .initialized();

    let outbox = alice.outbox().collect::<Vec<_>>();
    outbox
        .iter()
        .find(|o| matches!(o, Io::Connect(a, _) if *a == bob.id()))
        .unwrap();
    outbox
        .iter()
        .find(|o| matches!(o, Io::Connect(a, _) if *a == eve.id()))
        .unwrap();
}

#[test]
fn test_inventory_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::with_storage(
        "alice",
        [7, 7, 7, 7],
        Storage::open(tmp.path().join("alice"), fixtures::user()).unwrap(),
    );
    let bob_signer = Device::mock();
    let bob_storage = fixtures::storage(tmp.path().join("bob"), &bob_signer).unwrap();
    let bob = Peer::with_storage("bob", [8, 8, 8, 8], bob_storage);
    let now = LocalTime::now().into();
    let repos = bob.inventory().into_iter().collect::<Vec<_>>();

    alice.connect_to(&bob);
    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: repos.clone().try_into().unwrap(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );

    for proj in &repos {
        let seeds = alice.database().routing().get(proj).unwrap();
        assert!(seeds.contains(&bob.node_id()));
    }
}

#[test]
fn test_inventory_pruning() {
    struct Test {
        limits: Limits,
        /// Number of projects by peer
        peer_projects: Vec<usize>,
        wait_time: LocalDuration,
        expected_routing_table_size: usize,
    }
    let tests = [
        // All zero
        Test {
            limits: Limits {
                routing_max_size: 0,
                routing_max_age: LocalDuration::from_secs(0),
                ..Limits::default()
            },
            peer_projects: vec![10; 5],
            wait_time: LocalDuration::from_mins(7 * 24 * 60) + LocalDuration::from_secs(1),
            expected_routing_table_size: 0,
        },
        // All entries are too young to expire.
        Test {
            limits: Limits {
                routing_max_size: 0,
                routing_max_age: LocalDuration::from_mins(7 * 24 * 60),
                ..Limits::default()
            },
            peer_projects: vec![10; 5],
            wait_time: LocalDuration::from_mins(7 * 24 * 60) + LocalDuration::from_secs(1),
            expected_routing_table_size: 0,
        },
        // All entries remain because the table is unconstrained.
        Test {
            limits: Limits {
                routing_max_size: 50,
                routing_max_age: LocalDuration::from_mins(0),
                ..Limits::default()
            },
            peer_projects: vec![10; 5],
            wait_time: LocalDuration::from_mins(7 * 24 * 60) + LocalDuration::from_secs(1),
            expected_routing_table_size: 50,
        },
        // Some entries are pruned because the table is constrained.
        Test {
            limits: Limits {
                routing_max_size: 25,
                routing_max_age: LocalDuration::from_mins(7 * 24 * 60),
                ..Limits::default()
            },
            peer_projects: vec![10; 5],
            wait_time: LocalDuration::from_mins(7 * 24 * 60) + LocalDuration::from_secs(1),
            expected_routing_table_size: 25,
        },
    ];

    for test in tests {
        let mut alice = Peer::config(
            "alice",
            [7, 7, 7, 7],
            MockStorage::empty(),
            peer::Config {
                config: Config {
                    limits: test.limits,
                    ..Config::new(node::Alias::new("alice"))
                },
                ..peer::Config::default()
            },
        )
        .initialized();

        let bob = Peer::config(
            "bob",
            [8, 8, 8, 8],
            MockStorage::empty(),
            peer::Config {
                local_time: alice.local_time(),
                ..peer::Config::default()
            },
        )
        .initialized();

        // Tell Alice about the amazing projects available
        alice.connect_to(&bob);
        for num_projs in test.peer_projects {
            let peer = Peer::new("other", [9, 9, 9, 9]);

            alice.receive(bob.id(), peer.node_announcement());
            alice.receive(
                bob.id(),
                Message::inventory(
                    InventoryAnnouncement {
                        inventory: test::arbitrary::vec::<RepoId>(num_projs)
                            .try_into()
                            .unwrap(),
                        timestamp: bob.local_time().into(),
                    },
                    peer.signer(),
                ),
            );
        }

        // Wait for things to happen
        assert!(test.wait_time > PRUNE_INTERVAL, "pruning must be triggered");
        alice.elapse(test.wait_time);

        assert_eq!(
            test.expected_routing_table_size,
            alice.database().routing().len().unwrap()
        );
    }
}

#[test]
fn test_seeding() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let proj_id: identity::RepoId = test::arbitrary::gen(1);

    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::Seed(proj_id, policy::Scope::default(), sender));
    let policy_change = receiver.recv().map_err(runtime::HandleError::from).unwrap();
    assert!(policy_change);
    assert!(alice.policies().is_seeding(&proj_id).unwrap());

    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::Unseed(proj_id, sender));
    let policy_change = receiver.recv().map_err(runtime::HandleError::from).unwrap();
    assert!(policy_change);
    assert!(!alice.policies().is_seeding(&proj_id).unwrap());
}

#[test]
fn test_inventory_relay_bad_timestamp() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let two_hours = 3600 * 1000 * 2;
    let timestamp = alice.timestamp() + two_hours;

    alice.connect_to(&bob);
    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: BoundedVec::new(),
                timestamp,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.outbox().next(),
        Some(Io::Disconnect(addr, DisconnectReason::Session(session::Error::InvalidTimestamp(t))))
        if addr == bob.id() && t == timestamp
    );
}

#[test]
fn test_announcement_rebroadcast() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);

    alice.connect_to(&bob);
    alice.connect_from(&eve);
    alice.outbox().for_each(drop);

    log::debug!(target: "test", "Receiving gossips..");

    let received = test::gossip::messages(6, alice.local_time(), MAX_TIME_DELTA);
    for msg in received.iter().cloned() {
        alice.receive(bob.id(), msg);
    }

    alice.receive(
        eve.id(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }),
    );

    let relayed = alice.messages(eve.id()).collect::<BTreeSet<_>>();
    let received = received
        .into_iter()
        .chain(Some(bob.node_announcement()))
        .collect::<BTreeSet<_>>();

    assert_eq!(relayed.len(), received.len());
    assert_eq!(relayed, received);
}

#[test]
fn test_announcement_rebroadcast_duplicates() {
    let mut carol = Peer::new("carol", [4, 4, 4, 4]);
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let rids = arbitrary::set::<RepoId>(3..=3);

    carol.init();
    alice.connect_to(&bob);
    alice.receive(bob.id, carol.node_announcement());

    // These are not expected to be relayed.
    let stale = {
        let mut anns = BTreeSet::new();

        for _ in 0..5 {
            carol.elapse(LocalDuration::from_mins(1));

            anns.insert(carol.inventory_announcement());
            anns.insert(carol.node_announcement());
        }
        anns
    };

    // These are expected to be relayed.
    let expected = {
        let mut anns = BTreeSet::new();

        carol.elapse(LocalDuration::from_mins(1));
        anns.insert(carol.inventory_announcement());
        anns.insert(carol.node_announcement());
        anns.insert(bob.node_announcement());

        for rid in rids {
            alice.seed(&rid, policy::Scope::All).unwrap();
            anns.insert(carol.refs_announcement(rid));
            anns.insert(bob.refs_announcement(rid));
        }
        anns
    };

    let mut all = stale.iter().chain(expected.iter()).collect::<Vec<_>>();
    fastrand::shuffle(&mut all);

    // Alice receives all messages out of order.
    for ann in all {
        alice.receive(bob.id, ann.clone());
    }

    // Alice relays just the expected ones back to Eve.
    alice.connect_from(&eve);
    alice.receive(
        eve.id(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }),
    );

    let relayed = alice.messages(eve.id()).collect::<BTreeSet<_>>();

    assert_eq!(relayed.len(), 9);
    assert_eq!(relayed, expected);
}

#[test]
fn test_announcement_rebroadcast_timestamp_filtered() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);

    alice.connect_to(&bob);

    let delta = LocalDuration::from_mins(10);
    let first = test::gossip::messages(3, alice.local_time() - delta, LocalDuration::from_secs(0));
    let second = test::gossip::messages(3, alice.local_time(), LocalDuration::from_secs(0));
    let third = test::gossip::messages(3, alice.local_time() + delta, LocalDuration::from_secs(0));

    // Alice receives three batches of messages.
    for msg in first
        .iter()
        .chain(second.iter())
        .chain(third.iter())
        .cloned()
    {
        alice.receive(bob.id(), msg);
    }

    // Eve subscribes to messages within the period of the second batch only.
    alice.connect_from(&eve);
    alice.receive(
        eve.id(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: alice.local_time().into(),
            until: (alice.local_time() + delta).into(),
        }),
    );

    let relayed = alice.relayed(eve.id()).collect::<BTreeSet<_>>();
    let second = second
        .into_iter()
        .chain(Some(bob.node_announcement()))
        .collect::<BTreeSet<_>>();

    assert_eq!(relayed.len(), second.len());
    assert_eq!(relayed, second);
}

#[test]
fn test_announcement_relay() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let mut bob = Peer::new("bob", [8, 8, 8, 8]);
    let mut eve = Peer::new("eve", [9, 9, 9, 9]);

    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice
        .receive(bob.id(), bob.inventory_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_))
    );

    alice.receive(bob.id(), bob.inventory_announcement());
    assert!(
        alice.messages(eve.id()).next().is_none(),
        "Another inventory with the same timestamp is ignored"
    );

    bob.elapse(LocalDuration::from_mins(1));
    alice
        .receive(bob.id(), bob.inventory_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "Another inventory with a fresher timestamp is relayed"
    );

    alice
        .receive(bob.id(), bob.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "A node announcement with the same timestamp as the inventory is relayed"
    );

    alice
        .receive(bob.id(), bob.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert!(alice.messages(eve.id()).next().is_none(), "Only once");

    alice
        .receive(eve.id(), eve.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(bob.id()).next(),
        Some(Message::Announcement(_)),
        "A node announcement from Eve is relayed to Bob"
    );
    assert!(
        alice.messages(eve.id()).next().is_none(),
        "But not back to Eve"
    );

    eve.elapse(LocalDuration::from_mins(1));
    alice
        .receive(bob.id(), eve.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert!(
        alice.messages(bob.id()).next().is_none(),
        "Bob already know about this message, since he sent it"
    );
    assert!(
        alice.messages(eve.id()).next().is_none(),
        "Eve already know about this message, since she signed it"
    );
}

#[test]
fn test_refs_announcement_relay_public() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], MockStorage::empty());
    let eve = Peer::with_storage(
        "eve",
        [8, 8, 8, 8],
        Storage::open(tmp.path().join("eve"), fixtures::user()).unwrap(),
    );

    let bob = {
        let mut rng = fastrand::Rng::new();
        let signer = Device::mock_rng(&mut rng);
        let storage = fixtures::storage(tmp.path().join("bob"), &signer).unwrap();

        Peer::config(
            "bob",
            [9, 9, 9, 9],
            storage,
            peer::Config {
                signer,
                rng,
                ..peer::Config::default()
            },
        )
        .initialized()
    };
    let bob_inv = bob.inventory().into_iter().collect::<Vec<_>>();

    alice.seed(&bob_inv[0], policy::Scope::All).unwrap();
    alice.seed(&bob_inv[1], policy::Scope::All).unwrap();
    alice.seed(&bob_inv[2], policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.receive(eve.id(), Message::Subscribe(Subscribe::all()));
    alice
        .receive(bob.id(), bob.refs_announcement(bob_inv[0]))
        .elapse(service::GOSSIP_INTERVAL);

    // Pretend Alice cloned Bob's repos.
    let repos = gen::<[MockRepository; 3]>(1);
    for (i, mut repo) in repos.into_iter().enumerate() {
        repo.doc.doc = repo
            .doc
            .doc
            .with_edits(|doc| {
                doc.visibility = Visibility::Public; // Public repos are always gossiped.
            })
            .unwrap();
        alice.storage_mut().repos.insert(bob_inv[i], repo);
    }
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "A refs announcement from Bob is relayed to Eve"
    );

    alice
        .receive(bob.id(), bob.refs_announcement(bob_inv[0]))
        .elapse(service::GOSSIP_INTERVAL);
    assert!(
        alice.messages(eve.id()).next().is_none(),
        "The same ref announement is not relayed"
    );

    alice
        .receive(bob.id(), bob.refs_announcement(bob_inv[1]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "But a different one is"
    );

    alice
        .receive(bob.id(), bob.refs_announcement(bob_inv[2]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "And a third one is as well"
    );
}

#[test]
fn test_refs_announcement_relay_private() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], MockStorage::empty());
    let eve = Peer::with_storage(
        "eve",
        [8, 8, 8, 8],
        Storage::open(tmp.path().join("eve"), fixtures::user()).unwrap(),
    );

    let bob = {
        let mut rng = fastrand::Rng::new();
        let signer = Device::mock_rng(&mut rng);
        let storage = fixtures::storage(tmp.path().join("bob"), &signer).unwrap();

        Peer::config(
            "bob",
            [9, 9, 9, 9],
            storage,
            peer::Config {
                signer,
                rng,
                ..peer::Config::default()
            },
        )
        .initialized()
    };
    let bob_inv = bob.inventory().into_iter().collect::<Vec<_>>();

    alice.seed(&bob_inv[0], policy::Scope::All).unwrap();
    alice.seed(&bob_inv[1], policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.receive(eve.id(), Message::Subscribe(Subscribe::all()));

    // The first repo is not visible to Eve.
    let repo1 = {
        let mut repo = gen::<MockRepository>(1);
        repo.doc.doc = repo
            .doc
            .doc
            .with_edits(|doc| {
                doc.visibility = Visibility::Private { allow: [].into() };
            })
            .unwrap();
        repo
    };
    alice.storage_mut().repos.insert(bob_inv[0], repo1);

    // The second repo is visible to Eve.
    let repo2 = {
        let mut repo = gen::<MockRepository>(1);
        repo.doc.doc = repo
            .doc
            .doc
            .with_edits(|doc| {
                doc.visibility = Visibility::Private {
                    allow: [eve.id.into()].into(),
                };
            })
            .unwrap();
        repo
    };
    alice.storage_mut().repos.insert(bob_inv[1], repo2);
    alice.elapse(service::GOSSIP_INTERVAL);
    alice.messages(eve.id()).for_each(drop);
    alice
        .receive(bob.id(), bob.refs_announcement(bob_inv[0]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        None,
        "The first ref announcement is not relayed to Eve"
    );

    alice
        .receive(bob.id(), bob.refs_announcement(bob_inv[1]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(Announcement {
            message: AnnouncementMessage::Refs(_),
            ..
        })),
        "The second ref announcement is relayed to Eve"
    );
}

/// Even if Alice is not tracking Bob, Alice will fetch Bob's refs for a repo she doesn't have.
#[test]
fn test_refs_announcement_fetch_trusted_no_inventory() {
    logger::init(log::Level::Debug);

    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::with_storage(
        "alice",
        [7, 7, 7, 7],
        Storage::open(tmp.path().join("alice"), fixtures::user()).unwrap(),
    );
    let bob = {
        let mut rng = fastrand::Rng::new();
        let signer = Device::mock_rng(&mut rng);
        let storage = fixtures::storage(tmp.path().join("bob"), &signer).unwrap();

        Peer::config(
            "bob",
            [9, 9, 9, 9],
            storage,
            peer::Config {
                signer,
                rng,
                ..peer::Config::default()
            },
        )
        .initialized()
    };
    let bob_inv = bob.inventory();
    let rid = bob_inv.iter().next().unwrap();

    alice.seed(rid, policy::Scope::Followed).unwrap();
    alice.connect_to(&bob);

    // Alice receives Bob's refs.
    alice.receive(bob.id(), bob.refs_announcement(*rid));

    // Alice fetches Bob's refs as this is a new repo.
    assert_matches!(alice.outbox().next(), Some(Io::Fetch { .. }));
}

/// Alice and Bob both have the same repo.
///
/// First, Alice will not fetch from Bob's `RefsAnnouncement` as Alice does not
/// track Bob as `Followed`.
///
/// Later Alice follows Bob, and will be able to fetch Bob's refs.
#[test]
fn test_refs_announcement_followed() {
    logger::init(log::Level::Debug);

    // Create MockStorage for Alice and Bob. Both will have repo with `rid`.
    let storage_alice = arbitrary::nonempty_storage(1);
    let rid = *storage_alice.repos.keys().next().unwrap();
    let storage_bob = storage_alice.clone();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], storage_alice);
    let mut bob = Peer::with_storage("bob", [8, 8, 8, 8], storage_bob);

    let node_id = alice.id;
    let repo = alice.storage_mut().repo_mut(&rid);

    repo.remotes.insert(
        node_id,
        bob.signed_refs_at(arbitrary::gen::<Refs>(8), arbitrary::oid(), repo),
    );

    // Generate some refs for Bob under their own node_id.
    let sigrefs = bob.signed_refs_at(arbitrary::gen::<Refs>(8), arbitrary::oid(), repo);
    let node_id = bob.id;
    bob.init();
    bob.storage_mut()
        .repo_mut(&rid)
        .remotes
        .insert(node_id, sigrefs);

    // Alice uses Scope::Followed, and did not track Bob yet.
    alice.connect_to(&bob);
    alice.seed(&rid, policy::Scope::Followed).unwrap();

    // Alice receives Bob's refs
    alice.receive(bob.id(), bob.refs_announcement(rid));

    // Alice does not fetch as Alice is not tracking Bob.
    assert!(
        alice.messages(bob.id()).next().is_none(),
        "Alice is not tracking bob yet."
    );

    // Alice starts to track Bob.
    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::Follow(
        bob.id,
        Some(node::Alias::new("bob")),
        sender,
    ));
    let policy_change = receiver.recv().map_err(runtime::HandleError::from).unwrap();
    assert!(policy_change);

    // Bob announces refs again.
    bob.elapse(LocalDuration::from_mins(1)); // Make sure our announcement is fresh.
    alice.receive(bob.id(), bob.refs_announcement(rid));
    assert_matches!(alice.outbox().next(), Some(Io::Fetch { .. }));
}

#[test]
fn test_refs_announcement_no_subscribe() {
    let storage = arbitrary::nonempty_storage(1);
    let rid = *storage.repos.keys().next().unwrap();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], storage);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let id = arbitrary::gen(1);

    alice.seed(&id, policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.receive(bob.id(), bob.refs_announcement(rid));

    assert!(alice.messages(eve.id()).next().is_none());
}

#[test]
fn test_refs_announcement_offline() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = {
        let signer = Device::mock();
        let storage = fixtures::storage(tmp.path().join("alice"), &signer).unwrap();

        Peer::config(
            "alice",
            [7, 7, 7, 7],
            storage,
            peer::Config {
                signer,
                ..peer::Config::default()
            },
        )
    };
    let mut bob = Peer::new("bob", [8, 8, 8, 8]);

    // Make sure alice's service wasn't initialized before.
    assert_eq!(*alice.clock(), LocalTime::default());

    alice.initialize();
    alice.connect_to(&bob);
    alice.receive(bob.id, Message::Subscribe(Subscribe::all()));

    let mut inv = alice.inventory();
    let rid = *inv.iter().next().unwrap();

    bob.seed(&rid, policy::Scope::All).unwrap();

    // Alice announces the refs of all projects since she hasn't announced refs for these projects
    // yet.
    for msg in alice.messages(bob.id()) {
        assert_matches!(
            msg,
            Message::Announcement(Announcement {
                node,
                message: AnnouncementMessage::Refs(RefsAnnouncement {
                    rid,
                    ..
                }),
                ..
            })
            if node == alice.id && inv.remove(&rid)
        );
    }

    // Create an issue without telling the node.
    let repo = alice.storage().repository(rid).unwrap();
    let old_refs = RefsAt::new(&repo, alice.id).unwrap();
    let mut issues = radicle::issue::Cache::no_cache(&repo).unwrap();
    issues
        .create("Issue while offline!", "", &[], &[], [], alice.signer())
        .unwrap();
    let new_refs = RefsAt::new(&repo, alice.id).unwrap();
    assert_ne!(old_refs, new_refs);

    // Now we restart Alice's node. It should pick up that something's changed in storage.
    alice.elapse(LocalDuration::from_secs(60));
    alice
        .database_mut()
        .addresses_mut()
        .remove(&bob.id)
        .unwrap(); // Make sure we don't reconnect automatically.
    alice.disconnected(
        bob.id,
        Link::Outbound,
        &DisconnectReason::Session(session::Error::Timeout),
    );
    alice.outbox().for_each(drop);
    alice.restart();
    alice.connect_to(&bob);
    alice.receive(
        bob.id,
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: alice.timestamp(),
            until: Timestamp::MAX,
        }),
    );

    let anns = alice
        .messages(bob.id())
        .filter_map(|m| {
            if let Message::Announcement(Announcement {
                message: AnnouncementMessage::Refs(ann),
                ..
            }) = m
            {
                Some(ann)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(anns.len(), 1);
    assert_eq!(anns.first().unwrap().rid, rid);
    assert_eq!(anns.first().unwrap().refs.first().unwrap().at, new_refs.at);
}

#[test]
fn test_inventory_relay() {
    logger::init(log::Level::Debug);

    // Topology is eve <-> alice <-> bob
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let inv = BoundedVec::try_from(arbitrary::vec(1)).unwrap();
    let now = LocalTime::now().into();

    // Inventory from Bob relayed to Eve.
    alice.init();
    alice.wake(); // Run all periodic tasks now so they don't trigger later.
    alice.connect_to(&bob);
    alice.connect_from(&eve);
    alice
        .receive(
            bob.id(),
            Message::inventory(
                InventoryAnnouncement {
                    inventory: inv.clone(),
                    timestamp: now,
                },
                bob.signer(),
            ),
        )
        .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        alice.inventory_announcements(eve.id()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == bob.node_id() && timestamp == now
    );
    assert_matches!(
        alice.inventory_announcements(bob.id()).next(),
        None,
        "The inventory is not sent back to Bob"
    );

    alice
        .receive(
            bob.id(),
            Message::inventory(
                InventoryAnnouncement {
                    inventory: inv.clone(),
                    timestamp: now,
                },
                bob.signer(),
            ),
        )
        .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        alice.inventory_announcements(eve.id()).next(),
        None,
        "Sending the same inventory again doesn't trigger a relay"
    );

    alice
        .receive(
            bob.id(),
            Message::inventory(
                InventoryAnnouncement {
                    inventory: inv.clone(),
                    timestamp: now + 1,
                },
                bob.signer(),
            ),
        )
        .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        alice.inventory_announcements(eve.id()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == bob.node_id() && timestamp == now + 1,
        "Sending a new inventory does trigger the relay"
    );

    // Inventory from Eve relayed to Bob.
    alice
        .receive(
            eve.id(),
            Message::inventory(
                InventoryAnnouncement {
                    inventory: inv,
                    timestamp: now,
                },
                eve.signer(),
            ),
        )
        .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        alice.inventory_announcements(bob.id()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == eve.node_id() && timestamp == now
    );
}

#[test]
fn test_persistent_peer_reconnect_attempt() {
    use std::collections::HashSet;

    let mut bob = Peer::new("bob", [8, 8, 8, 8]);
    let mut eve = Peer::new("eve", [9, 9, 9, 9]);
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        MockStorage::empty(),
        peer::Config {
            config: Config {
                connect: HashSet::from_iter([
                    (bob.id(), bob.address()).into(),
                    (eve.id(), eve.address()).into(),
                ]),
                ..Config::new(node::Alias::new("alice"))
            },
            ..peer::Config::default()
        },
    )
    .initialized();

    let mut sim = Simulation::new(
        LocalTime::now(),
        alice.rng.clone(),
        simulator::Options::default(),
    )
    .initialize([&mut alice, &mut bob, &mut eve]);

    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    let ips = alice
        .sessions()
        .connected()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert!(ips.contains(&bob.id()));
    assert!(ips.contains(&eve.id()));

    // ... Negotiated ...
    //
    // Now let's disconnect a peer.

    // A non-transient disconnect, such as one due to peer misbehavior will still trigger a
    // a reconnection, since this is a persistent peer.
    let reason = DisconnectReason::Session(session::Error::Misbehavior);

    for _ in 0..3 {
        alice.disconnected(bob.id(), Link::Outbound, &reason);
        alice.elapse(service::MAX_RECONNECTION_DELTA);
        alice
            .outbox()
            .find(|io| matches!(io, Io::Connect(a, _) if a == &bob.id()))
            .unwrap();

        alice.attempted(bob.id(), bob.address());
    }
}

#[test]
fn test_persistent_peer_reconnect_success() {
    use std::collections::HashSet;

    let bob = Peer::with_storage("bob", [9, 9, 9, 9], MockStorage::empty());
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        MockStorage::empty(),
        peer::Config {
            config: Config {
                connect: HashSet::from_iter([(bob.id, bob.addr()).into()]),
                ..Config::new(node::Alias::new("alice"))
            },
            ..peer::Config::default()
        },
    )
    .initialized();
    alice.connect_to(&bob);

    // A transient error such as this will cause Alice to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));
    alice.disconnected(
        bob.id(),
        Link::Outbound,
        &DisconnectReason::Connection(error),
    );
    alice.elapse(service::MIN_RECONNECTION_DELTA);
    alice.elapse(service::MIN_RECONNECTION_DELTA); // Trigger a second wakeup to test idempotence.

    alice
        .outbox()
        .find_map(|o| match o {
            Io::Connect(id, _) => Some(id),
            _ => None,
        })
        .expect("Alice attempts a re-connection");

    alice.attempted(bob.id(), bob.addr());
    alice.connected(bob.id(), bob.addr(), Link::Outbound);
}

#[test]
fn test_maintain_connections() {
    // Peers alice starts out connected to.
    let connected = vec![
        Peer::new("connected", [8, 8, 8, 1]),
        Peer::new("connected", [8, 8, 8, 2]),
        Peer::new("connected", [8, 8, 8, 3]),
    ];
    // Peers alice will connect to once the others disconnect.
    let mut unconnected = vec![
        Peer::new("unconnected", [9, 9, 9, 1]),
        Peer::new("unconnected", [9, 9, 9, 2]),
        Peer::new("unconnected", [9, 9, 9, 3]),
    ];

    let mut alice = Peer::new("alice", [7, 7, 7, 7]);

    for peer in connected.iter() {
        alice.connect_to(peer);
    }
    assert_eq!(
        connected.len(),
        alice.sessions().len(),
        "alice should be connected to the first set of peers"
    );
    // We now import the other addresses.
    alice.import_addresses(&unconnected);

    // A non-transient error such as this will cause Alice to attempt a different peer.
    let error = session::Error::Misbehavior;
    for peer in connected.iter() {
        alice.disconnected(peer.id(), Link::Outbound, &DisconnectReason::Session(error));

        let id = alice
            .outbox()
            .find_map(|o| match o {
                Io::Connect(id, _) => Some(id),
                _ => None,
            })
            .expect("Alice connects to a new peer");
        assert_ne!(id, peer.id());
        unconnected.retain(|p| p.id() != id);
    }
    assert!(
        unconnected.is_empty(),
        "alice should connect to all unconnected peers"
    );
}

#[test]
fn test_maintain_connections_transient() {
    // Peers alice starts out connected to.
    let connected = vec![
        Peer::new("connected", [8, 8, 8, 1]),
        Peer::new("connected", [8, 8, 8, 2]),
        Peer::new("connected", [8, 8, 8, 3]),
    ];
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);

    for peer in connected.iter() {
        alice.connect_to(peer);
    }
    // A transient error such as this will cause Alice to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));
    for peer in connected.iter() {
        alice.disconnected(
            peer.id(),
            Link::Outbound,
            &DisconnectReason::Connection(error.clone()),
        );
        alice
            .outbox()
            .find(|o| matches!(o, Io::Connect(id, _) if id == &peer.id()))
            .unwrap();
    }
}

#[test]
fn test_maintain_connections_failed_attempt() {
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let reason =
        DisconnectReason::Connection(Arc::new(io::Error::from(io::ErrorKind::ConnectionReset)));

    // Make sure Alice knows about Eve.
    alice.connect_to(&eve);
    alice.disconnected(eve.id(), Link::Outbound, &reason);
    alice
        .outbox()
        .find(|o| matches!(o, Io::Connect(id, _) if id == &eve.id))
        .expect("Alice attempts Eve");
    alice.attempted(eve.id, eve.addr());

    // Disconnect Eve and make sure Alice doesn't try to re-connect immediately.
    alice.disconnected(eve.id(), Link::Outbound, &reason);
    assert_matches!(
        alice.outbox().find(|o| matches!(o, Io::Connect(_, _))),
        None
    );

    // Now pass some time and try again.
    alice.elapse(MAX_RECONNECTION_DELTA);
    alice
        .outbox()
        .find(|o| matches!(o, Io::Connect(id, _) if id == &eve.id))
        .expect("Alice attempts Eve again");

    // Disconnect Eve and make sure Alice doesn't try to re-connect immediately.
    alice.disconnected(eve.id(), Link::Outbound, &reason);
    assert!(!alice.outbox().any(|o| matches!(o, Io::Connect(_, _))));
    // Or even after some short time..
    alice.elapse(MIN_RECONNECTION_DELTA);
    assert!(!alice.outbox().any(|o| matches!(o, Io::Connect(_, _))));
}

#[test]
fn test_seed_repo_subscribe() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let rid = arbitrary::gen::<RepoId>(1);
    let (send, recv) = chan::bounded(1);

    alice.connect_to(&bob);
    alice.command(Command::Seed(rid, policy::Scope::default(), send));
    assert!(recv.recv().unwrap());

    assert_matches!(
        alice.messages(bob.id).next(),
        Some(Message::Subscribe(Subscribe {
            filter,
            since,
            ..
        })) if since == alice.timestamp() && filter.contains(&rid)
    );
}

#[test]
fn test_fetch_missing_inventory_on_gossip() {
    let rid = arbitrary::gen::<RepoId>(1);
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let now = LocalTime::now();

    alice.seed(&rid, node::policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: vec![rid].try_into().unwrap(),
                timestamp: now.into(),
            },
            bob.signer(),
        ),
    );
    alice
        .outbox()
        .find(|m| matches!(m, Io::Fetch { rid: other, .. } if other == &rid))
        .unwrap();
}

#[test]
fn test_fetch_missing_inventory_on_schedule() {
    let rid = arbitrary::gen::<RepoId>(1);
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let now = LocalTime::now();

    alice.seed(&rid, node::policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: vec![rid].try_into().unwrap(),
                timestamp: now.into(),
            },
            bob.signer(),
        ),
    );
    alice.fetched(
        rid,
        bob.id,
        Err(worker::FetchError::Io(
            io::ErrorKind::ConnectionReset.into(),
        )),
    );
    alice.outbox().for_each(drop);
    alice.elapse(service::SYNC_INTERVAL);
    alice
        .outbox()
        .find(|m| matches!(m, Io::Fetch { rid: other, .. } if other == &rid))
        .unwrap();
}

#[test]
fn test_queued_fetch_max_capacity() {
    let storage = arbitrary::nonempty_storage(3);
    let mut repo_keys = storage.repos.keys();
    let rid1 = *repo_keys.next().unwrap();
    let rid2 = *repo_keys.next().unwrap();
    let rid3 = *repo_keys.next().unwrap();
    let doc = storage.repos.get(&rid1).unwrap().doc.clone();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], storage);
    let bob = Peer::new("bob", [8, 8, 8, 8]);

    logger::init(log::Level::Debug);

    alice.connect_to(&bob);

    // Send the first fetch.
    let (send, _recv1) = chan::bounded::<node::FetchResult>(1);
    alice.command(Command::Fetch(rid1, bob.id, DEFAULT_TIMEOUT, send));

    // Send the 2nd fetch that will be queued.
    let (send2, _recv2) = chan::bounded::<node::FetchResult>(1);
    alice.command(Command::Fetch(rid2, bob.id, DEFAULT_TIMEOUT, send2));

    // Send the 3rd fetch that will be queued.
    let (send3, _recv3) = chan::bounded::<node::FetchResult>(1);
    alice.command(Command::Fetch(rid3, bob.id, DEFAULT_TIMEOUT, send3));

    // The first fetch is initiated.
    assert_matches!(alice.fetches().next(), Some((rid, _)) if rid == rid1);
    // We shouldn't send out the 2nd, 3rd fetch while we're doing the 1st fetch.
    assert_matches!(alice.outbox().next(), None);

    // Have enough time pass that Alice sends a "ping" to Bob.
    alice.elapse(KEEP_ALIVE_DELTA);

    // Finish the 1st fetch.
    alice.fetched(rid1, bob.id, Ok(fetch::FetchResult::new(doc.clone())));
    // Now the 1st fetch is done, the 2nd fetch is dequeued.
    assert_matches!(alice.fetches().next(), Some((rid, _)) if rid == rid2);
    // ... but not the third.
    assert_matches!(alice.fetches().next(), None);

    // Finish the 2nd fetch.
    alice.fetched(rid2, bob.id, Ok(fetch::FetchResult::new(doc)));
    // Now the 2nd fetch is done, the 3rd fetch is dequeued.
    assert_matches!(alice.fetches().next(), Some((rid, _)) if rid == rid3);
}

#[test]
fn test_queued_fetch_from_ann_same_rid() {
    let storage = arbitrary::nonempty_storage(1); // We're testing both public and private repos.
    let mut repo_keys = storage.repos.keys();
    let rid = *repo_keys.next().unwrap();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], storage);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let carol = Peer::new("carol", [10, 10, 10, 10]);
    let oid = arbitrary::oid();
    let ann = RefsAnnouncement {
        rid,
        refs: vec![RefsAt {
            remote: carol.id(),
            at: oid,
        }]
        .try_into()
        .unwrap(),
        timestamp: bob.timestamp(),
    };

    alice.seed(&rid, policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.connect_to(&carol);

    // Send the first announcement.
    alice.receive(bob.id, bob.announcement(ann.clone()));
    // Send the 2nd announcement that will be queued.
    alice.receive(eve.id, eve.announcement(ann.clone()));
    // Send the 3rd announcement that will be queued.
    alice.receive(carol.id, carol.announcement(ann));

    // The first fetch is initiated.
    assert_matches!(alice.fetches().next(), Some((rid_, nid_)) if rid_ == rid && nid_ == bob.id);
    // We shouldn't send out the 2nd, 3rd fetch while we're doing the 1st fetch.
    assert_matches!(alice.fetches().next(), None);

    // Have enough time pass that Alice sends a "ping" to Bob.
    alice.elapse(KEEP_ALIVE_DELTA);

    let refname = carol
        .id()
        .to_namespace()
        .join(git::refname!("refs/sigrefs"));

    // Finish the 1st fetch.
    // Ensure the ref is in the storage and cache.
    let repo = alice.storage_mut().repo_mut(&rid);
    repo.remotes.insert(
        carol.id(),
        carol.signed_refs_at(arbitrary::gen::<Refs>(1), oid, repo),
    );
    alice
        .database_mut()
        .refs_mut()
        .set(&rid, &carol.id, &SIGREFS_BRANCH, oid, LocalTime::now())
        .unwrap();
    alice.fetched(
        rid,
        bob.id,
        Ok(fetch::FetchResult {
            updated: vec![RefUpdate::Created {
                name: refname.clone(),
                oid,
            }],
            namespaces: [carol.id()].into_iter().collect(),
            clone: false,
            doc: arbitrary::gen(1),
        }),
    );
    // Now the 1st fetch is done, but the 2nd and 3rd fetches are redundant.
    assert_matches!(alice.fetches().next(), None);
}

#[test]
fn test_queued_fetch_from_command_same_rid() {
    let storage = arbitrary::nonempty_storage(3);
    let mut repo_keys = storage.repos.keys();
    let rid1 = *repo_keys.next().unwrap();
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], storage);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let carol = Peer::new("carol", [10, 10, 10, 10]);

    logger::init(log::Level::Debug);

    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.connect_to(&carol);

    // Send the first fetch.
    let (send, _recv1) = chan::bounded::<node::FetchResult>(1);
    alice.command(Command::Fetch(rid1, bob.id, DEFAULT_TIMEOUT, send));

    // Send the 2nd fetch that will be queued.
    let (send2, _recv2) = chan::bounded::<node::FetchResult>(1);
    alice.command(Command::Fetch(rid1, eve.id, DEFAULT_TIMEOUT, send2));

    // Send the 3rd fetch that will be queued.
    let (send3, _recv3) = chan::bounded::<node::FetchResult>(1);
    alice.command(Command::Fetch(rid1, carol.id, DEFAULT_TIMEOUT, send3));

    // Peers Alice will fetch from.
    let mut peers = [bob.id, eve.id, carol.id]
        .into_iter()
        .collect::<BTreeSet<_>>();

    // The first fetch is initiated.
    let (rid, nid) = alice.fetches().next().unwrap();
    assert_eq!(rid, rid1);
    assert!(peers.remove(&nid));

    // We shouldn't send out the 2nd, 3rd fetch while we're doing the 1st fetch.
    assert_matches!(alice.outbox().next(), None);

    // Have enough time pass that Alice sends a "ping" to Bob.
    alice.elapse(KEEP_ALIVE_DELTA);

    // Finish the 1st fetch.
    alice.fetched(rid1, nid, Ok(arbitrary::gen::<fetch::FetchResult>(1)));
    // Now the 1st fetch is done, the 2nd fetch is dequeued.
    let (rid, nid) = alice.fetches().next().unwrap();
    assert_eq!(rid, rid1);
    assert!(peers.remove(&nid));

    // ... but not the third.
    assert_matches!(alice.fetches().next(), None);

    // Finish the 2nd fetch.
    alice.fetched(rid1, nid, Ok(arbitrary::gen::<fetch::FetchResult>(1)));
    // Now the 2nd fetch is done, the 3rd fetch is dequeued.
    assert_matches!(alice.fetches().next(), Some((rid, nid)) if rid == rid1 && peers.remove(&nid));
    // All fetches were initiated.
    assert!(peers.is_empty());
}

#[test]
fn test_refs_synced_event() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path(), fixtures::user()).unwrap();
    let mut alice = Peer::with_storage("alice", [8, 8, 8, 8], storage.clone());
    let bob = Peer::new("bob", [9, 9, 9, 9]);
    let eve = Peer::with_storage("eve", [7, 7, 7, 7], storage);
    let acme = alice.project("acme", "");
    let events = alice.events();
    let ann = AnnouncementMessage::from(RefsAnnouncement {
        rid: acme,
        refs: vec![RefsAt::new(&alice.storage().repository(acme).unwrap(), alice.id).unwrap()]
            .try_into()
            .unwrap(),
        timestamp: bob.timestamp(),
    });
    let msg = ann.signed(bob.signer());

    alice.seed(&acme, policy::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.receive(bob.id, Message::Announcement(msg));

    events
        .wait(
            |e| {
                matches!(
                    e,
                    Event::RefsSynced { remote, rid, .. }
                    if rid == &acme && remote == &bob.id
                )
                .then_some(())
            },
            time::Duration::from_secs(3),
        )
        .unwrap();

    // Now a relayed announcement.
    alice.receive(bob.id, eve.node_announcement());
    alice.receive(bob.id, eve.refs_announcement(acme));

    events
        .wait(
            |e| matches!(e, Event::RefsSynced { remote, .. } if remote == &eve.id).then_some(()),
            time::Duration::from_secs(3),
        )
        .unwrap();
}

#[test]
fn test_init_and_seed() {
    let tempdir = tempfile::tempdir().unwrap();

    let storage_alice = Storage::open(
        tempdir.path().join("alice").join("storage"),
        fixtures::user(),
    )
    .unwrap();
    let (repo, _) = fixtures::repository(tempdir.path().join("working"));
    let mut alice = Peer::with_storage("alice", [7, 7, 7, 7], storage_alice);

    let storage_bob =
        Storage::open(tempdir.path().join("bob").join("storage"), fixtures::user()).unwrap();
    let mut bob = Peer::with_storage("bob", [8, 8, 8, 8], storage_bob);

    let storage_eve =
        Storage::open(tempdir.path().join("eve").join("storage"), fixtures::user()).unwrap();
    let mut eve = Peer::with_storage("eve", [9, 9, 9, 9], storage_eve);

    remote::mock::register(&alice.node_id(), alice.storage().path());
    remote::mock::register(&eve.node_id(), eve.storage().path());
    remote::mock::register(&bob.node_id(), bob.storage().path());
    local::register(alice.storage().clone());

    // Alice and Bob connect to Eve.
    alice.command(service::Command::Connect(
        eve.id(),
        eve.address(),
        ConnectOptions::default(),
    ));
    bob.command(service::Command::Connect(
        eve.id(),
        eve.address(),
        ConnectOptions::default(),
    ));

    // Alice creates a new project.
    let (proj_id, _, _) = rad::init(
        &repo,
        "alice".try_into().unwrap(),
        "alice's repo",
        git::refname!("master"),
        Visibility::default(),
        alice.signer(),
        alice.storage(),
    )
    .unwrap();

    let mut sim = Simulation::new(
        LocalTime::now(),
        alice.rng.clone(),
        simulator::Options::default(),
    )
    .initialize([&mut alice, &mut bob, &mut eve]);

    let bob_events = bob.events();

    // Neither Eve nor Bob have Alice's project for now.
    assert!(eve.get(proj_id).unwrap().is_none());
    assert!(bob.get(proj_id).unwrap().is_none());

    // Bob seeds Alice's project.
    let (sender, receiver) = chan::bounded(1);
    bob.command(service::Command::Seed(
        proj_id,
        policy::Scope::default(),
        sender,
    ));
    assert!(receiver.recv().unwrap());

    // Eve seeds Alice's project.
    let (sender, receiver) = chan::bounded(1);
    eve.command(service::Command::Seed(
        proj_id,
        policy::Scope::default(),
        sender,
    ));
    assert!(receiver.recv().unwrap());

    let (send, _) = chan::bounded(1);
    // Alice announces her inventory.
    // We now expect Eve to fetch Alice's project from Alice.
    // Then we expect Bob to fetch Alice's project from Eve.
    alice.elapse(LocalDuration::from_secs(1)); // Make sure our announcement is fresh.
    alice.command(service::Command::AddInventory(proj_id, send));

    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    log::debug!(target: "test", "Simulation is over");

    // TODO: Refs should be compared between the two peers.

    log::debug!(target: "test", "Waiting for {} to fetch {} from {}..", bob.id, proj_id,eve.id);
    bob_events
        .iter()
        .find(|e| {
            matches!(
                e,
                service::Event::RefsFetched { remote, .. }
                if *remote == eve.node_id()
            )
        })
        .expect("Bob fetched from Eve");

    assert!(eve.storage().get(proj_id).unwrap().is_some());
    assert!(bob.storage().get(proj_id).unwrap().is_some());
}

#[test]
fn prop_inventory_exchange_dense() {
    fn property(alice_inv: MockStorage, bob_inv: MockStorage, eve_inv: MockStorage) {
        let rng = fastrand::Rng::new();
        let alice = Peer::with_storage(
            "alice",
            [7, 7, 7, 7],
            alice_inv
                .clone()
                .map(|doc| doc.visibility = Visibility::Public),
        );
        let mut bob = Peer::with_storage(
            "bob",
            [8, 8, 8, 8],
            bob_inv
                .clone()
                .map(|doc| doc.visibility = Visibility::Public),
        );
        let mut eve = Peer::with_storage(
            "eve",
            [9, 9, 9, 9],
            eve_inv
                .clone()
                .map(|doc| doc.visibility = Visibility::Public),
        );
        let mut routing = RandomMap::with_hasher(rng.clone().into());

        for (inv, peer) in &[
            (alice_inv.repos, alice.node_id()),
            (bob_inv.repos, bob.node_id()),
            (eve_inv.repos, eve.node_id()),
        ] {
            for id in inv.keys() {
                routing
                    .entry(*id)
                    .or_insert_with(|| RandomSet::with_hasher(rng.clone().into()))
                    .insert(*peer);
            }
        }

        // Fully-connected.
        bob.command(Command::Connect(
            alice.id(),
            alice.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            eve.id(),
            eve.address(),
            ConnectOptions::default(),
        ));
        eve.command(Command::Connect(
            alice.id(),
            alice.address(),
            ConnectOptions::default(),
        ));

        let mut peers: RandomMap<_, _> = [
            (alice.node_id(), alice),
            (bob.node_id(), bob),
            (eve.node_id(), eve),
        ]
        .into_iter()
        .collect();
        let mut simulator = Simulation::new(LocalTime::now(), rng, simulator::Options::default())
            .initialize(peers.values_mut());

        simulator.run_while(peers.values_mut(), |s| !s.is_settled());

        for (proj_id, remotes) in &routing {
            for peer in peers.values() {
                let lookup = peer.lookup(*proj_id).unwrap();

                if lookup.local.is_some() {
                    peer.get(*proj_id)
                        .expect("There are no errors querying storage")
                        .expect("The project is available locally");
                } else {
                    for remote in &lookup.remote {
                        peers[remote]
                            .get(*proj_id)
                            .expect("There are no errors querying storage")
                            .expect("The project is available remotely");
                    }
                    assert!(
                        !lookup.remote.is_empty(),
                        "There are remote locations for the project"
                    );
                    assert_eq!(
                        &lookup.remote.into_iter().collect::<RandomSet<_>>(),
                        remotes,
                        "The remotes match the global routing table"
                    );
                }
            }
        }
    }
    qcheck::QuickCheck::new()
        .gen(qcheck::Gen::new(5))
        .tests(20)
        .quickcheck(property as fn(MockStorage, MockStorage, MockStorage));
}

#[test]
fn test_announcement_message_amplification() {
    let mut results = Vec::new();
    let mut rng = fastrand::Rng::new();

    while results.len() < *TEST_CASES {
        let mut alice = Peer::new("alice", [7, 7, 7, 7]);
        let mut bob = Peer::new("bob", [8, 8, 8, 8]);
        let mut eve = Peer::new("eve", [9, 9, 9, 9]);
        let mut zod = Peer::new("zod", [5, 5, 5, 5]);
        let mut tom = Peer::new("tom", [4, 4, 4, 4]);
        let mut sim = Simulation::new(
            LocalTime::now(),
            alice.rng.clone(),
            simulator::Options {
                latency: 0..1, // 0 - 1s
                failure_rate: 0.,
            },
        );
        let rid = gen::<RepoId>(1);

        // Make sure the node gossip intervals are not accidentally synchronized.
        alice.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        bob.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        eve.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        zod.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        tom.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));

        // Fully-connected network.
        alice.command(Command::Connect(
            bob.id,
            bob.address(),
            ConnectOptions::default(),
        ));
        alice.command(Command::Connect(
            eve.id,
            eve.address(),
            ConnectOptions::default(),
        ));
        alice.command(Command::Connect(
            zod.id,
            zod.address(),
            ConnectOptions::default(),
        ));
        alice.command(Command::Connect(
            tom.id,
            tom.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            eve.id,
            eve.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            zod.id,
            zod.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            tom.id,
            tom.address(),
            ConnectOptions::default(),
        ));
        eve.command(Command::Connect(
            zod.id,
            zod.address(),
            ConnectOptions::default(),
        ));
        eve.command(Command::Connect(
            tom.id,
            tom.address(),
            ConnectOptions::default(),
        ));
        zod.command(Command::Connect(
            tom.id,
            tom.address(),
            ConnectOptions::default(),
        ));

        // Let the nodes connect to each other.
        sim.run_while([&mut alice, &mut bob, &mut eve, &mut zod, &mut tom], |s| {
            s.elapsed() < LocalDuration::from_mins(3)
        });

        // Ensure nodes are all connected, otherwise skip this test run.
        if alice.sessions().connected().count() != 4 {
            continue;
        }
        if bob.sessions().connected().count() != 4 {
            continue;
        }
        if eve.sessions().connected().count() != 4 {
            continue;
        }
        if zod.sessions().connected().count() != 4 {
            continue;
        }
        if tom.sessions().connected().count() != 4 {
            continue;
        }

        let (tx, _) = chan::bounded(1);
        let timestamp = (*alice.clock()).into();
        alice
            .storage_mut()
            .repos
            .insert(rid, gen::<MockRepository>(1));
        alice.command(Command::AddInventory(rid, tx));

        sim.run_while([&mut alice, &mut bob, &mut eve, &mut zod, &mut tom], |s| {
            s.elapsed() < LocalDuration::from_mins(3)
        });

        // Make sure they have the routing table entry.
        for node in [&bob, &eve, &zod, &tom] {
            assert!(node
                .service
                .database()
                .routing()
                .get(&rid)
                .unwrap()
                .contains(&alice.id));
        }

        // Count how many copies of Alice's inventory message have been received by peers.
        let received = sim.messages().iter().filter(|m| {
            matches!(
                m,
                (_, _, Message::Announcement(Announcement {
                    node,
                    message: AnnouncementMessage::Inventory(i),
                    ..
                }))
                if node == &alice.id && i.inventory.to_vec() == vec![rid] && i.timestamp == timestamp
            )
        });
        results.push(received.count());
    }
    // Calculate the average amplification factor based on all simulation runs.
    let avg = results.iter().sum::<usize>() as f64 / results.len() as f64;
    // Amplification is total divided by minimum, ie. it's a relative metric.
    let amp = avg / 4.;

    // The worse case scenario is (n - 1)^2 messages received for one message announced.
    // In the above case of 5 nodes, this is 4 * 4 = 16 messages. This is an amplification of 4.0.
    // The best case is an amplification of 1.0, ie. each node receives the message once only.
    //
    // By using delayed message propagation though, we can bring this down closer to the minimum.
    log::debug!(target: "test", "Average message amplification: {amp}");

    assert!(amp < 2., "Amplification factor of {amp} is too high");
    assert!(amp >= 1., "Amplification can't be lower than 1");
}
