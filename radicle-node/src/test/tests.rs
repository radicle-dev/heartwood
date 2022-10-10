use std::io;
use std::sync::Arc;

use crossbeam_channel as chan;
use nakamoto_net as nakamoto;

use crate::collections::{HashMap, HashSet};
use crate::prelude::{LocalDuration, Timestamp};
use crate::service::config::*;
use crate::service::filter::Filter;
use crate::service::message::*;
use crate::service::peer::*;
use crate::service::reactor::Io;
use crate::service::ServiceState as _;
use crate::service::*;
use crate::storage::git::Storage;
use crate::storage::ReadStorage;
use crate::test::assert_matches;
use crate::test::fixtures;
#[allow(unused)]
use crate::test::logger;
use crate::test::peer::Peer;
use crate::test::signer::MockSigner;
use crate::test::simulator;
use crate::test::simulator::{Peer as _, Simulation};
use crate::test::storage::MockStorage;
use crate::LocalTime;
use crate::{client, git, identity, rad, service, test};

// NOTE
//
// If you wish to see the logs for a running test, simply add the following line to your test:
//
//      logger::init(log::Level::Debug);
//
// You may then run the test with eg. `cargo test -- --nocapture` to always show output.

#[test]
fn test_outbound_connection() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8], MockStorage::empty());
    let bob = Peer::new("bob", [9, 9, 9, 9], MockStorage::empty());
    let eve = Peer::new("eve", [7, 7, 7, 7], MockStorage::empty());

    alice.connect_to(&bob);
    alice.connect_to(&eve);

    let peers = alice
        .service
        .sessions()
        .negotiated()
        .map(|(ip, _)| *ip)
        .collect::<Vec<_>>();

    assert!(peers.contains(&eve.ip));
    assert!(peers.contains(&bob.ip));
}

#[test]
fn test_inbound_connection() {
    let mut alice = Peer::new("alice", [8, 8, 8, 8], MockStorage::empty());
    let bob = Peer::new("bob", [9, 9, 9, 9], MockStorage::empty());
    let eve = Peer::new("eve", [7, 7, 7, 7], MockStorage::empty());

    alice.connect_from(&bob);
    alice.connect_from(&eve);

    let peers = alice
        .service
        .sessions()
        .negotiated()
        .map(|(ip, _)| *ip)
        .collect::<Vec<_>>();

    assert!(peers.contains(&eve.ip));
    assert!(peers.contains(&bob.ip));
}

#[test]
fn test_persistent_peer_connect() {
    let rng = fastrand::Rng::new();
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());
    let config = Config {
        connect: vec![bob.address(), eve.address()],
        ..Config::default()
    };
    let mut alice = Peer::config(
        "alice",
        config,
        [7, 7, 7, 7],
        vec![],
        MockStorage::empty(),
        rng,
    );

    alice.initialize();

    let mut outbox = alice.outbox();
    assert_matches!(outbox.next(), Some(Io::Connect(a)) if a == bob.addr());
    assert_matches!(outbox.next(), Some(Io::Connect(a)) if a == eve.addr());
    assert_matches!(outbox.next(), None);
}

#[test]
#[ignore]
fn test_wrong_peer_version() {
    // TODO
}

#[test]
#[ignore]
fn test_wrong_peer_magic() {
    // TODO
}

#[test]
fn test_inventory_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::new(
        "alice",
        [7, 7, 7, 7],
        Storage::open(tmp.path().join("alice")).unwrap(),
    );
    let bob_signer = MockSigner::default();
    let bob_storage = fixtures::storage(tmp.path().join("bob"), bob_signer).unwrap();
    let bob = Peer::new("bob", [8, 8, 8, 8], bob_storage);
    let now = LocalTime::now().as_secs();
    let projs = bob.storage().inventory().unwrap();

    alice.connect_to(&bob);
    alice.receive(
        &bob.addr(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: projs.clone(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );

    for proj in &projs {
        let seeds = alice.routing().get(proj).unwrap();
        assert!(seeds.contains(&bob.node_id()));
    }

    let a = alice
        .storage()
        .inventory()
        .unwrap()
        .into_iter()
        .collect::<HashSet<_>>();
    let b = projs.into_iter().collect::<HashSet<_>>();

    assert_eq!(a, b);
}

#[test]
fn test_tracking() {
    let mut alice = Peer::config(
        "alice",
        Config {
            project_tracking: ProjectTracking::Allowed(HashSet::default()),
            ..Config::default()
        },
        [7, 7, 7, 7],
        vec![],
        MockStorage::empty(),
        fastrand::Rng::new(),
    );
    let proj_id: identity::Id = test::arbitrary::gen(1);

    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::Track(proj_id, sender));
    let policy_change = receiver
        .recv()
        .map_err(client::handle::Error::from)
        .unwrap();
    assert!(policy_change);
    assert!(alice.config().is_tracking(&proj_id));

    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::Untrack(proj_id, sender));
    let policy_change = receiver
        .recv()
        .map_err(client::handle::Error::from)
        .unwrap();
    assert!(policy_change);
    assert!(!alice.config().is_tracking(&proj_id));
}

#[test]
fn test_inventory_relay_bad_timestamp() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7], MockStorage::empty());
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let two_hours = 3600 * 2;
    let timestamp = alice.local_time.as_secs() + two_hours;

    alice.connect_to(&bob);
    alice.receive(
        &bob.addr(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: vec![],
                timestamp,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.outbox().next(),
        Some(Io::Disconnect(addr, DisconnectReason::Error(SessionError::InvalidTimestamp(t))))
        if addr == bob.addr() && t == timestamp
    );
}

#[test]
fn test_gossip_rebroadcast() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7], MockStorage::empty());
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());

    alice.connect_to(&bob);

    let received = test::gossip::messages(6, alice.local_time(), MAX_TIME_DELTA);
    for msg in received.iter().cloned() {
        alice.receive(&bob.addr(), msg);
    }

    alice.connect_from(&eve);
    alice.receive(
        &eve.addr(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }),
    );

    let relayed = alice.messages(&eve.addr()).collect::<Vec<_>>();
    assert_eq!(relayed, received);
}

#[test]
fn test_gossip_rebroadcast_timestamp_filtered() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7], MockStorage::empty());
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());

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
        alice.receive(&bob.addr(), msg);
    }

    // Eve subscribes to messages within the period of the second batch only.
    alice.connect_from(&eve);
    alice.receive(
        &eve.addr(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: alice.local_time().as_secs(),
            until: (alice.local_time() + delta).as_secs(),
        }),
    );

    let relayed = alice.messages(&eve.addr()).collect::<Vec<_>>();
    assert_eq!(relayed.len(), second.len());
    assert_eq!(relayed, second);
}

#[test]
fn test_inventory_relay() {
    // Topology is eve <-> alice <-> bob
    let mut alice = Peer::new("alice", [7, 7, 7, 7], MockStorage::empty());
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());
    let inv = vec![];
    let now = LocalTime::now().as_secs();

    // Inventory from Bob relayed to Eve.
    alice.connect_to(&bob);
    alice.connect_from(&eve);
    alice.receive(
        &bob.addr(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.messages(&eve.addr()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == bob.node_id() && timestamp == now
    );
    assert_matches!(
        alice.messages(&bob.addr()).next(),
        None,
        "The inventory is not sent back to Bob"
    );

    alice.receive(
        &bob.addr(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.messages(&eve.addr()).next(),
        None,
        "Sending the same inventory again doesn't trigger a relay"
    );

    alice.receive(
        &bob.addr(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now + 1,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.messages(&eve.addr()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == bob.node_id() && timestamp == now + 1,
        "Sending a new inventory does trigger the relay"
    );

    // Inventory from Eve relayed to Bob.
    alice.receive(
        &eve.addr(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv,
                timestamp: now,
            },
            eve.signer(),
        ),
    );
    assert_matches!(
        alice.messages(&bob.addr()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == eve.node_id() && timestamp == now
    );
}

#[test]
fn test_persistent_peer_reconnect() {
    let mut bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let mut eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());
    let mut alice = Peer::config(
        "alice",
        Config {
            connect: vec![bob.address(), eve.address()],
            ..Config::default()
        },
        [7, 7, 7, 7],
        vec![],
        MockStorage::empty(),
        fastrand::Rng::new(),
    );

    let mut sim = Simulation::new(
        LocalTime::now(),
        alice.rng.clone(),
        simulator::Options::default(),
    )
    .initialize([&mut alice, &mut bob, &mut eve]);

    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    let ips = alice
        .sessions()
        .negotiated()
        .map(|(ip, _)| *ip)
        .collect::<Vec<_>>();
    assert!(ips.contains(&bob.ip));
    assert!(ips.contains(&eve.ip));

    // ... Negotiated ...
    //
    // Now let's disconnect a peer.

    // A transient error such as this will cause Alice to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));

    // A non-transient disconnect, such as one requested by the user will not trigger
    // a reconnection.
    alice.disconnected(
        &eve.addr(),
        &nakamoto::DisconnectReason::DialError(error.clone()),
    );
    assert_matches!(alice.outbox().next(), None);

    for _ in 0..MAX_CONNECTION_ATTEMPTS {
        alice.disconnected(
            &bob.addr(),
            &nakamoto::DisconnectReason::ConnectionError(error.clone()),
        );
        assert_matches!(alice.outbox().next(), Some(Io::Connect(a)) if a == bob.addr());
        assert_matches!(alice.outbox().next(), None);

        alice.attempted(&bob.addr());
    }

    // After the max connection attempts, a disconnect doesn't trigger a reconnect.
    alice.disconnected(
        &bob.addr(),
        &nakamoto::DisconnectReason::ConnectionError(error),
    );
    assert_matches!(alice.outbox().next(), None);
}

#[test]
fn test_push_and_pull() {
    logger::init(log::Level::Debug);

    let tempdir = tempfile::tempdir().unwrap();

    let storage_alice = Storage::open(tempdir.path().join("alice").join("storage")).unwrap();
    let (repo, _) = fixtures::repository(tempdir.path().join("working"));
    let mut alice = Peer::new("alice", [7, 7, 7, 7], storage_alice);

    let storage_bob = Storage::open(tempdir.path().join("bob").join("storage")).unwrap();
    let mut bob = Peer::new("bob", [8, 8, 8, 8], storage_bob);

    let storage_eve = Storage::open(tempdir.path().join("eve").join("storage")).unwrap();
    let mut eve = Peer::new("eve", [9, 9, 9, 9], storage_eve);

    // Alice and Bob connect to Eve.
    alice.command(service::Command::Connect(eve.addr()));
    bob.command(service::Command::Connect(eve.addr()));

    let mut sim = Simulation::new(
        LocalTime::now(),
        alice.rng.clone(),
        simulator::Options::default(),
    )
    .initialize([&mut alice, &mut bob, &mut eve]);

    // Here we expect Alice to connect to Eve.
    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    // Alice creates a new project.
    let (proj_id, _) = rad::init(
        &repo,
        "alice",
        "alice's repo",
        git::refname!("master"),
        alice.signer(),
        alice.storage(),
    )
    .unwrap();

    // Bob tracks Alice's project.
    let (sender, _) = chan::bounded(1);
    bob.command(service::Command::Track(proj_id, sender));

    // Eve tracks Alice's project.
    let (sender, _) = chan::bounded(1);
    eve.command(service::Command::Track(proj_id, sender));

    // Neither of them have it in the beginning.
    assert!(eve.get(proj_id).unwrap().is_none());
    assert!(bob.get(proj_id).unwrap().is_none());

    // Alice announces her refs.
    // We now expect Eve to fetch Alice's project from Alice.
    // Then we expect Bob to fetch Alice's project from Eve.
    alice.command(service::Command::AnnounceRefs(proj_id));
    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    assert!(eve
        .storage()
        .get(&alice.node_id(), proj_id)
        .unwrap()
        .is_some());
    assert!(bob
        .storage()
        .get(&alice.node_id(), proj_id)
        .unwrap()
        .is_some());
    assert_matches!(
        sim.events(&bob.ip).next(),
        Some(service::Event::RefsFetched { from, .. })
        if from == eve.git_url(),
        "Bob fetched from Eve"
    );
}

#[test]
fn prop_inventory_exchange_dense() {
    fn property(alice_inv: MockStorage, bob_inv: MockStorage, eve_inv: MockStorage) {
        let rng = fastrand::Rng::new();
        let alice = Peer::new("alice", [7, 7, 7, 7], alice_inv.clone());
        let mut bob = Peer::new("bob", [8, 8, 8, 8], bob_inv.clone());
        let mut eve = Peer::new("eve", [9, 9, 9, 9], eve_inv.clone());
        let mut routing = HashMap::with_hasher(rng.clone().into());

        for (inv, peer) in &[
            (alice_inv.inventory, alice.node_id()),
            (bob_inv.inventory, bob.node_id()),
            (eve_inv.inventory, eve.node_id()),
        ] {
            for id in inv.keys() {
                routing
                    .entry(*id)
                    .or_insert_with(|| HashSet::with_hasher(rng.clone().into()))
                    .insert(*peer);
            }
        }

        // Fully-connected.
        bob.command(Command::Connect(alice.addr()));
        bob.command(Command::Connect(eve.addr()));
        eve.command(Command::Connect(alice.addr()));
        eve.command(Command::Connect(bob.addr()));

        let mut peers: HashMap<_, _> = [
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
                        &lookup.remote.into_iter().collect::<HashSet<_>>(),
                        remotes,
                        "The remotes match the global routing table"
                    );
                }
            }
        }
    }
    quickcheck::QuickCheck::new()
        .gen(quickcheck::Gen::new(8))
        .quickcheck(property as fn(MockStorage, MockStorage, MockStorage));
}
