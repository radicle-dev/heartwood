mod e2e;

use std::default::*;
use std::io;
use std::sync::Arc;

use crossbeam_channel as chan;
use netservices::LinkDirection as Link;

use crate::collections::{HashMap, HashSet};
use crate::crypto::test::signer::MockSigner;
use crate::identity::Id;
use crate::prelude::*;
use crate::prelude::{LocalDuration, Timestamp};
use crate::service::config::*;
use crate::service::filter::Filter;
use crate::service::message::*;
use crate::service::reactor::Io;
use crate::service::ServiceState as _;
use crate::service::*;
use crate::storage::git::transport::{local, remote};
use crate::storage::git::Storage;
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
use crate::LocalTime;
use crate::{git, identity, rad, runtime, service, test};

// NOTE
//
// If you wish to see the logs for a running test, simply add the following line to your test:
//
//      logger::init(log::Level::Debug);
//
// You may then run the test with eg. `cargo test -- --nocapture` to always show output.

#[test]
fn test_inventory_decode() {
    let inventory: Vec<Id> = arbitrary::gen(300);
    let timestamp = LocalTime::now().as_secs();

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
    assert_eq!(1, alice.sessions().negotiated().count(), "bob connects");
    alice.elapse(STALE_CONNECTION_TIMEOUT + LocalDuration::from_secs(1));
    alice
        .outbox()
        .find(|m| matches!(m, &Io::Disconnect(addr, _) if addr == bob.id()))
        .expect("disconnect an unresponsive bob");
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

    alice.command(service::Command::Connect(bob.id(), bob.address()));
    sim.run_while([&mut alice, &mut bob], |s| !s.is_settled());
    assert_eq!(1, alice.sessions().negotiated().count(), "bob connects");

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
        .negotiated()
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
        .negotiated()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    assert!(peers.contains(&eve.id()));
    assert!(peers.contains(&bob.id()));
}

#[test]
fn test_persistent_peer_connect() {
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        MockStorage::empty(),
        peer::Config {
            config: Config {
                connect: vec![(bob.id(), bob.address()), (eve.id(), eve.address())],
                ..Config::default()
            },
            ..peer::Config::default()
        },
    );

    alice.initialize();

    let mut outbox = alice.outbox();
    assert_matches!(outbox.next(), Some(Io::Connect(a, _)) if a == bob.id());
    assert_matches!(outbox.next(), Some(Io::Connect(a, _)) if a == eve.id());
    assert_matches!(outbox.next(), None);
}

#[test]
fn test_inventory_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        Storage::open(tmp.path().join("alice")).unwrap(),
        peer::Config::default(),
    );
    let bob_signer = MockSigner::default();
    let bob_storage = fixtures::storage(tmp.path().join("bob"), &bob_signer).unwrap();
    let bob = Peer::config("bob", [8, 8, 8, 8], bob_storage, peer::Config::default());
    let now = LocalTime::now().as_secs();
    let projs = bob.storage().inventory().unwrap();

    alice.connect_to(&bob);
    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: projs.clone().try_into().unwrap(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );

    for proj in &projs {
        let seeds = alice.routing().get(proj).unwrap();
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
                    ..Config::default()
                },
                ..peer::Config::default()
            },
        );

        let bob = Peer::config(
            "bob",
            [8, 8, 8, 8],
            MockStorage::empty(),
            peer::Config {
                local_time: alice.local_time(),
                ..peer::Config::default()
            },
        );

        // Tell Alice about the amazing projects available
        alice.connect_to(&bob);
        for num_projs in test.peer_projects {
            alice.receive(
                bob.id(),
                Message::inventory(
                    InventoryAnnouncement {
                        inventory: test::arbitrary::vec::<Id>(num_projs).try_into().unwrap(),
                        timestamp: bob.local_time().as_secs(),
                    },
                    &MockSigner::default(),
                ),
            );
        }

        // Wait for things to happen
        assert!(test.wait_time > PRUNE_INTERVAL, "pruning must be triggered");
        alice.elapse(test.wait_time);

        assert_eq!(
            test.expected_routing_table_size,
            alice.routing().len().unwrap()
        );
    }
}

#[test]
fn test_tracking() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let proj_id: identity::Id = test::arbitrary::gen(1);

    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::TrackRepo(proj_id, sender));
    let policy_change = receiver.recv().map_err(runtime::HandleError::from).unwrap();
    assert!(policy_change);
    assert!(alice.tracking().is_repo_tracked(&proj_id).unwrap());

    let (sender, receiver) = chan::bounded(1);
    alice.command(Command::UntrackRepo(proj_id, sender));
    let policy_change = receiver.recv().map_err(runtime::HandleError::from).unwrap();
    assert!(policy_change);
    assert!(!alice.tracking().is_repo_tracked(&proj_id).unwrap());
}

#[test]
fn test_inventory_relay_bad_timestamp() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let two_hours = 3600 * 2;
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

    let received = test::gossip::messages(6, alice.local_time(), MAX_TIME_DELTA);
    for msg in received.iter().cloned() {
        alice.receive(bob.id(), msg);
    }

    alice.connect_from(&eve);
    alice.receive(
        eve.id(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }),
    );

    let relayed = alice.messages(eve.id()).collect::<Vec<_>>();
    assert_eq!(relayed, received);
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
            since: alice.local_time().as_secs(),
            until: (alice.local_time() + delta).as_secs(),
        }),
    );

    let relayed = alice.messages(eve.id()).collect::<Vec<_>>();
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
    alice.receive(bob.id(), bob.inventory_announcement());

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
    alice.receive(bob.id(), bob.inventory_announcement());
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "Another inventory with a fresher timestamp is relayed"
    );

    alice.receive(bob.id(), bob.node_announcement());
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "A node announcement with the same timestamp as the inventory is relayed"
    );

    alice.receive(bob.id(), bob.node_announcement());
    assert!(alice.messages(eve.id()).next().is_none(), "Only once");

    alice.receive(eve.id(), eve.node_announcement());
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
    alice.receive(bob.id(), eve.node_announcement());
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
fn test_refs_announcement_relay() {
    let tmp = tempfile::tempdir().unwrap();
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        Storage::open(tmp.path().join("alice")).unwrap(),
        peer::Config::default(),
    );
    let eve = Peer::config(
        "eve",
        [8, 8, 8, 8],
        Storage::open(tmp.path().join("eve")).unwrap(),
        peer::Config::default(),
    );

    let bob = {
        let mut rng = fastrand::Rng::new();
        let signer = MockSigner::new(&mut rng);
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
    };
    let bob_inv = bob.inventory().unwrap();

    alice.track_repo(&bob_inv[0], tracking::Scope::All).unwrap();
    alice.track_repo(&bob_inv[1], tracking::Scope::All).unwrap();
    alice.track_repo(&bob_inv[2], tracking::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.receive(eve.id(), Message::Subscribe(Subscribe::all()));

    alice.receive(bob.id(), bob.refs_announcement(bob_inv[0]));
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "A refs announcement from Bob is relayed to Eve"
    );

    alice.receive(bob.id(), bob.refs_announcement(bob_inv[0]));
    assert!(
        alice.messages(eve.id()).next().is_none(),
        "The same ref announement is not relayed"
    );

    alice.receive(bob.id(), bob.refs_announcement(bob_inv[1]));
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "But a different one is"
    );

    alice.receive(bob.id(), bob.refs_announcement(bob_inv[2]));
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(_)),
        "And a third one is as well"
    );
}

#[test]
fn test_refs_announcement_no_subscribe() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let id = arbitrary::gen(1);

    alice.track_repo(&id, tracking::Scope::All).unwrap();
    alice.connect_to(&bob);
    alice.connect_to(&eve);
    alice.receive(bob.id(), bob.refs_announcement(id));

    assert!(alice.messages(eve.id()).next().is_none());
}

#[test]
fn test_inventory_relay() {
    // Topology is eve <-> alice <-> bob
    let mut alice = Peer::new("alice", [7, 7, 7, 7]);
    let bob = Peer::new("bob", [8, 8, 8, 8]);
    let eve = Peer::new("eve", [9, 9, 9, 9]);
    let inv = BoundedVec::try_from(arbitrary::vec(1)).unwrap();
    let now = LocalTime::now().as_secs();

    // Inventory from Bob relayed to Eve.
    alice.connect_to(&bob);
    alice.connect_from(&eve);
    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.messages(eve.id()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == bob.node_id() && timestamp == now
    );
    assert_matches!(
        alice.messages(bob.id()).next(),
        None,
        "The inventory is not sent back to Bob"
    );

    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.messages(eve.id()).next(),
        None,
        "Sending the same inventory again doesn't trigger a relay"
    );

    alice.receive(
        bob.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now + 1,
            },
            bob.signer(),
        ),
    );
    assert_matches!(
        alice.messages(eve.id()).next(),
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
        eve.id(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv,
                timestamp: now,
            },
            eve.signer(),
        ),
    );
    assert_matches!(
        alice.messages(bob.id()).next(),
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
    let mut bob = Peer::new("bob", [8, 8, 8, 8]);
    let mut eve = Peer::new("eve", [9, 9, 9, 9]);
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        MockStorage::empty(),
        peer::Config {
            config: Config {
                connect: vec![(bob.id(), bob.address()), (eve.id(), eve.address())],
                ..Config::default()
            },
            ..peer::Config::default()
        },
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
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert!(ips.contains(&bob.id()));
    assert!(ips.contains(&eve.id()));

    // ... Negotiated ...
    //
    // Now let's disconnect a peer.

    // A transient error such as this will cause Alice to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));

    // A non-transient disconnect, such as one requested by the user will not trigger
    // a reconnection.
    alice.disconnected(eve.id(), &DisconnectReason::Dial(error.clone()));
    assert_matches!(alice.outbox().next(), None);

    for _ in 0..MAX_CONNECTION_ATTEMPTS {
        alice.disconnected(bob.id(), &DisconnectReason::Connection(error.clone()));
        assert_matches!(alice.outbox().next(), Some(Io::Connect(a, _)) if a == bob.id());
        assert_matches!(alice.outbox().next(), None);

        alice.attempted(bob.id(), &bob.address());
    }

    // After the max connection attempts, a disconnect doesn't trigger a reconnect.
    alice.disconnected(bob.id(), &DisconnectReason::Connection(error));
    assert_matches!(alice.outbox().next(), None);
}

#[test]
fn test_persistent_peer_reconnect_success() {
    let bob = Peer::config(
        "bob",
        [9, 9, 9, 9],
        MockStorage::empty(),
        peer::Config::default(),
    );
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        MockStorage::empty(),
        peer::Config {
            config: Config {
                connect: vec![(bob.id, bob.addr())],
                ..Config::default()
            },
            ..peer::Config::default()
        },
    );

    alice.attempted(bob.id(), &bob.addr());
    alice.connected(bob.id(), Link::Outbound);

    // A transient error such as this will cause Alice to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));
    alice.disconnected(bob.id(), &DisconnectReason::Connection(error));

    alice
        .outbox()
        .find_map(|o| match o {
            Io::Connect(id, _) => Some(id),
            _ => None,
        })
        .expect("Alice attempts a re-connection");

    alice.attempted(bob.id(), &bob.addr());
    alice.connected(bob.id(), Link::Outbound);
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
    alice.import_addresses(&unconnected);

    for peer in connected.iter() {
        alice.connect_to(peer);
    }
    assert_eq!(
        connected.len(),
        alice.sessions().len(),
        "alice should be connected to all peers"
    );

    // A transient error such as this will cause Alice to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));
    for peer in connected.iter() {
        alice.disconnected(peer.id(), &DisconnectReason::Connection(error.clone()));

        let id = alice
            .outbox()
            .find_map(|o| match o {
                Io::Connect(id, _) => Some(id),
                _ => None,
            })
            .expect("Alice connects to a new peer");
        assert!(id != peer.id());
        unconnected.retain(|p| p.id() != id);
    }
    assert!(
        unconnected.is_empty(),
        "alice should connect to all unconnected peers"
    );
}

#[test]
fn test_push_and_pull() {
    let tempdir = tempfile::tempdir().unwrap();

    let storage_alice = Storage::open(tempdir.path().join("alice").join("storage")).unwrap();
    let (repo, _) = fixtures::repository(tempdir.path().join("working"));
    let mut alice = Peer::config(
        "alice",
        [7, 7, 7, 7],
        storage_alice,
        peer::Config::default(),
    );

    let storage_bob = Storage::open(tempdir.path().join("bob").join("storage")).unwrap();
    let mut bob = Peer::config("bob", [8, 8, 8, 8], storage_bob, peer::Config::default());

    let storage_eve = Storage::open(tempdir.path().join("eve").join("storage")).unwrap();
    let mut eve = Peer::config("eve", [9, 9, 9, 9], storage_eve, peer::Config::default());

    remote::mock::register(&alice.node_id(), alice.storage().path());
    remote::mock::register(&eve.node_id(), eve.storage().path());
    remote::mock::register(&bob.node_id(), bob.storage().path());
    local::register(alice.storage().clone());

    // Alice and Bob connect to Eve.
    alice.command(service::Command::Connect(eve.id(), eve.address()));
    bob.command(service::Command::Connect(eve.id(), eve.address()));

    // Alice creates a new project.
    let (proj_id, _, _) = rad::init(
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
    bob.command(service::Command::TrackRepo(proj_id, sender));

    // Eve tracks Alice's project.
    let (sender, _) = chan::bounded(1);
    eve.command(service::Command::TrackRepo(proj_id, sender));

    let mut sim = Simulation::new(
        LocalTime::now(),
        alice.rng.clone(),
        simulator::Options::default(),
    )
    .initialize([&mut alice, &mut bob, &mut eve]);

    // Here we expect Alice to connect to Eve.
    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    // Neither Eve nor Bob have Alice's project for now.
    assert!(eve.get(proj_id).unwrap().is_none());
    assert!(bob.get(proj_id).unwrap().is_none());

    // Alice announces her refs.
    // We now expect Eve to fetch Alice's project from Alice.
    // Then we expect Bob to fetch Alice's project from Eve.
    alice.elapse(LocalDuration::from_secs(1)); // Make sure our announcement is fresh.
    alice.command(service::Command::AnnounceRefs(proj_id));
    sim.run_while([&mut alice, &mut bob, &mut eve], |s| !s.is_settled());

    // TODO: Refs should be compared between the two peers.

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
        sim.events(&bob.id).next(),
        Some(service::Event::RefsFetched { remote, .. })
        if remote == eve.node_id(),
        "Bob fetched from Eve"
    );
}

#[test]
fn prop_inventory_exchange_dense() {
    fn property(alice_inv: MockStorage, bob_inv: MockStorage, eve_inv: MockStorage) {
        let rng = fastrand::Rng::new();
        let alice = Peer::config(
            "alice",
            [7, 7, 7, 7],
            alice_inv.clone(),
            peer::Config::default(),
        );
        let mut bob = Peer::config(
            "bob",
            [8, 8, 8, 8],
            bob_inv.clone(),
            peer::Config::default(),
        );
        let mut eve = Peer::config(
            "eve",
            [9, 9, 9, 9],
            eve_inv.clone(),
            peer::Config::default(),
        );
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
        bob.command(Command::Connect(alice.id(), alice.address()));
        bob.command(Command::Connect(eve.id(), eve.address()));
        eve.command(Command::Connect(alice.id(), alice.address()));

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
    qcheck::QuickCheck::new()
        .gen(qcheck::Gen::new(8))
        .quickcheck(property as fn(MockStorage, MockStorage, MockStorage));
}
