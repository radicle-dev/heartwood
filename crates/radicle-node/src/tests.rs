mod e2e;

use std::collections::BTreeSet;
use std::default::*;
use std::env;
use std::io;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time;

use radicle::storage::ReadRepository;
use test_log::test;

use localtime::LocalDuration;
use localtime::LocalTime;
use protocol::bounded::BoundedVec;
use protocol::service;
use protocol::service::ServiceState as _;
use protocol::service::filter::Filter;
use protocol::service::io::Io;
use protocol::service::message::*;
use protocol::service::*;
use protocol::wire::Decode;
use protocol::wire::Encode;
use protocol::worker::fetch::FetchResult;
use radicle::assert_matches;
use radicle::cob;
use radicle::collections::{RandomMap, RandomSet};
use radicle::crypto::SigningKey;
use radicle::identity::RepoId;
use radicle::identity::Visibility;
use radicle::node;
use radicle::node::Event;
use radicle::node::Link;
use radicle::node::Timestamp;
use radicle::node::address::Store as _;
use radicle::node::config::*;
use radicle::node::policy;
use radicle::node::refs::Store as _;
use radicle::node::routing::Store as _;
use radicle::node::{ConnectOptions, DEFAULT_TIMEOUT};
use radicle::storage::ReadStorage;
use radicle::storage::RefUpdate;
use radicle::storage::git::Storage;
use radicle::storage::git::transport::{local, remote};
use radicle::storage::refs::RefsAt;
use radicle::storage::refs::SIGREFS_BRANCH;
use radicle::test::arbitrary;
use radicle::test::arbitrary::r#gen;
use radicle::test::fixtures;
use radicle::test::storage::MockRepository;
use radicle::test::storage::MockStorage;
use radicle::{git, identity, rad};
#[allow(unused)]
use radicle_log::test as logger;

use crate::test::peer::Peer;
use crate::test::peer::{AMY, BOB, CID};
use crate::test::simulator;
use crate::test::simulator::Simulation;
use crate::{runtime, test};

/// Default number of tests to run when testing things with high variance.
pub const DEFAULT_TEST_CASES: usize = 10;
/// Test cases to run when testing things with high variance.
pub static TEST_CASES: LazyLock<usize> = LazyLock::new(|| {
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
fn inventory_decode() {
    let inventory: Vec<RepoId> = arbitrary::r#gen(300);
    let timestamp: Timestamp = LocalTime::now().into();

    let mut buf = Vec::new();
    inventory.as_slice().encode(&mut buf);
    timestamp.encode(&mut buf);

    let m = InventoryAnnouncement::decode(&mut buf.as_slice()).expect("message decodes");
    assert_eq!(inventory.as_slice(), m.inventory.as_slice());
    assert_eq!(timestamp, m.timestamp);
}

#[test]
fn ping_response() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let cid = Peer::cid();

    amy.connect_to(&bob);
    amy.receive(
        *bob.nid(),
        Message::Ping(Ping {
            ponglen: Ping::MAX_PONG_ZEROES,
            zeroes: ZeroBytes::new(42),
        }),
    );
    assert_matches!(
        amy.messages(*bob.nid()).next(),
        Some(Message::Pong { zeroes }) if zeroes.len() == Ping::MAX_PONG_ZEROES as usize,
        "respond with correctly formatted pong",
    );

    amy.connect_to(&cid);
    amy.receive(
        *cid.nid(),
        Message::Ping(Ping {
            ponglen: Ping::MAX_PONG_ZEROES + 1,
            zeroes: ZeroBytes::new(42),
        }),
    );
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        None,
        "ignore unsupported ping message",
    );
}

#[test]
fn disconnecting_unresponsive_peer() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();

    amy.connect_to(&bob);
    assert_eq!(1, amy.sessions().connected().count(), "bob connects");
    amy.elapse(STALE_CONNECTION_TIMEOUT + LocalDuration::from_secs(1));
    amy.outbox()
        .find(|m| matches!(m, &Io::Disconnect(addr, _) if addr == *bob.nid()))
        .expect("disconnect an unresponsive bob");
}

#[test]
fn redundant_connect() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let opts = ConnectOptions::default();

    amy.command(Command::Connect(*bob.nid(), bob.address(), opts.clone()));
    amy.command(Command::Connect(*bob.nid(), bob.address(), opts.clone()));
    amy.command(Command::Connect(*bob.nid(), bob.address(), opts));

    // Only one connection attempt is made.
    assert_matches!(
        amy.outbox().filter(|o| matches!(o, Io::Connect { .. })).collect::<Vec<_>>().as_slice(),
        [Io::Connect(id, addr)]
        if *id == *bob.nid() && *addr == bob.address()
    );
}

#[test]
fn connection_kept_alive() {
    let mut amy = Peer::amy();
    let mut bob = Peer::bob();

    let mut sim = Simulation::new(LocalTime::now(), simulator::Options::default());

    amy.command(service::Command::Connect(
        *bob.nid(),
        bob.address(),
        ConnectOptions::default(),
    ));
    sim.run_while([&mut amy, &mut bob], |s| !s.is_settled());
    assert_eq!(1, amy.sessions().connected().count(), "bob connects");

    let mut elapsed: LocalDuration = LocalDuration::from_secs(0);
    let step: LocalDuration = STALE_CONNECTION_TIMEOUT / 10;
    while elapsed < STALE_CONNECTION_TIMEOUT + step {
        amy.elapse(step);
        bob.elapse(step);
        sim.run_while([&mut amy, &mut bob], |s| !s.is_settled());

        elapsed = elapsed + step;
    }

    assert_eq!(1, amy.sessions().len(), "Amy remains connected to Bob");
    assert_eq!(1, bob.sessions().len(), "Bob remains connected to Amy");
}

#[test]
fn outbound_connection() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let cid = Peer::cid();

    amy.connect_to(&bob);
    amy.connect_to(&cid);

    let peers = amy
        .sessions()
        .connected()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    assert!(peers.contains(cid.nid()));
    assert!(peers.contains(bob.nid()));
}

#[test]
fn inbound_connection() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let cid = Peer::cid();

    amy.connect_from(&bob);
    amy.connect_from(&cid);

    let peers = amy
        .sessions()
        .connected()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    assert!(peers.contains(cid.nid()));
    assert!(peers.contains(bob.nid()));
}

#[test]
fn persistent_peer_connect() {
    use indexmap::IndexSet;

    let bob = Peer::bob();
    let cid = Peer::cid();
    let connect = IndexSet::<ConnectAddress>::from_iter([
        (*bob.nid(), bob.address()).into(),
        (*cid.nid(), cid.address()).into(),
    ]);

    let mut amy = Peer::amy_with(move |config| {
        config.config.connect = connect;
    });

    let outbox = amy.outbox().collect::<Vec<_>>();
    outbox
        .iter()
        .find(|o| matches!(o, Io::Connect(a, _) if *a == *bob.nid()))
        .unwrap();
    outbox
        .iter()
        .find(|o| matches!(o, Io::Connect(a, _) if *a == *cid.nid()))
        .unwrap();
}

#[test]
fn inventory_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let mut amy = Peer::with_storage(
        "amy",
        AMY,
        Storage::open(tmp.path().join("amy"), fixtures::user()).unwrap(),
    );
    let bob_secret = BOB;
    let bob_signer = SigningKey::mock(bob_secret as usize);
    let bob_storage = fixtures::storage(tmp.path().join("bob"), &bob_signer).unwrap();
    let bob = Peer::with_storage("bob", bob_secret, bob_storage);
    let now = LocalTime::now().into();
    let repos = bob.inventory().into_iter().collect::<Vec<_>>();

    amy.connect_to(&bob);
    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: repos.clone().try_into().unwrap(),
                timestamp: now,
            },
            bob.secret_key(),
        ),
    );

    for proj in &repos {
        let seeds = amy.database().routing().get(proj).unwrap();
        assert!(seeds.contains(bob.nid()));
    }
}

#[test]
fn inventory_pruning() {
    const ONE_WEEK: LocalDuration = LocalDuration::from_mins(7 * 24 * 60);
    const PROJECTS_PER_ITERATION: usize = 10;
    const ITERATIONS: usize = 5;
    const PROJECTS_TOTAL: usize = PROJECTS_PER_ITERATION * ITERATIONS;

    fn one_second_more(duration: LocalDuration) -> LocalDuration {
        duration + LocalDuration::from_secs(1)
    }

    assert!(
        one_second_more(ONE_WEEK) > PRUNE_INTERVAL,
        "pruning must be triggered"
    );

    struct Test {
        limits: Limits,
        expected_routing_table_size: usize,
    }
    let tests = [
        // All zero
        Test {
            limits: Limits {
                routing_max_size: 0.into(),
                routing_max_age: LocalDuration::from_secs(0).into(),
                ..Limits::default()
            },
            expected_routing_table_size: 0,
        },
        // All entries are too young to expire.
        Test {
            limits: Limits {
                routing_max_size: 0.into(),
                routing_max_age: ONE_WEEK.into(),
                ..Limits::default()
            },
            expected_routing_table_size: 0,
        },
        // Some entries are pruned because the table is constrained.
        Test {
            limits: Limits {
                routing_max_size: 5.into(),
                routing_max_age: ONE_WEEK.into(),
                ..Limits::default()
            },
            expected_routing_table_size: 5,
        },
        // All entries remain because the table constraints are so lax.
        Test {
            limits: Limits {
                routing_max_size: 25.into(),
                routing_max_age: ONE_WEEK.into(),
                ..Limits::default()
            },
            expected_routing_table_size: 25,
        },
    ];

    for test in tests {
        let mut amy = Peer::amy_with(move |config| {
            config.config.limits = test.limits;
        });

        let amy_local_time = amy.local_time();
        let bob = Peer::bob_with(move |config| {
            config.local_time = amy_local_time;
        });

        assert_eq!(bob.local_time(), amy.local_time());

        let projects: [RepoId; PROJECTS_TOTAL] =
            arbitrary::set::<RepoId>(PROJECTS_TOTAL..=PROJECTS_TOTAL)
                .into_iter()
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();

        // Tell Amy about the amazing projects available
        amy.connect_to(&bob);
        for i in 0..ITERATIONS {
            let peer = Peer::new_empty_storage("peer", (100 + i) as u8);

            amy.receive(*bob.nid(), peer.node_announcement());
            amy.receive(
                *bob.nid(),
                Message::inventory(
                    InventoryAnnouncement {
                        inventory: projects
                            [(i * PROJECTS_PER_ITERATION)..((i + 1) * PROJECTS_PER_ITERATION)]
                            .to_owned()
                            .try_into()
                            .unwrap(),
                        timestamp: bob.local_time().into(),
                    },
                    peer.secret_key(),
                ),
            );
        }

        // Wait for things to happen
        amy.elapse(one_second_more(ONE_WEEK));

        assert_eq!(
            test.expected_routing_table_size,
            amy.database().routing().len().unwrap()
        );
    }
}

#[test]
fn seeding() {
    let mut amy = Peer::amy();
    let proj_id: identity::RepoId = arbitrary::r#gen(1);

    let (cmd, receiver) = Command::seed(proj_id, policy::Scope::default());
    amy.command(cmd);
    let policy_change = receiver
        .recv()
        .map_err(runtime::handle::Error::from)
        .unwrap()
        .unwrap();
    assert!(policy_change);
    assert!(amy.policies().is_seeding(&proj_id).unwrap());

    let (cmd, receiver) = Command::unseed(proj_id);
    amy.command(cmd);
    let policy_change = receiver
        .recv()
        .map_err(runtime::handle::Error::from)
        .unwrap()
        .unwrap();
    assert!(policy_change);
    assert!(!amy.policies().is_seeding(&proj_id).unwrap());
}

#[test]
fn inventory_relay_bad_timestamp() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let two_hours = 3600 * 1000 * 2;
    let timestamp = Timestamp::from(*amy.clock()) + two_hours;

    amy.connect_to(&bob);
    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: BoundedVec::new(),
                timestamp,
            },
            bob.secret_key(),
        ),
    );
    assert_matches!(
        amy.outbox().next(),
        Some(Io::Disconnect(addr, DisconnectReason::Session(session::Error::InvalidTimestamp(session::InvalidTimestamp::Future { theirs, .. }))))
        if addr == *bob.nid() && theirs == timestamp
    );
}

#[test]
fn announcement_rebroadcast() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let cid = Peer::cid();

    amy.connect_to(&bob);
    amy.connect_from(&cid);
    amy.outbox().for_each(drop);

    log::debug!(target: "test", "Receiving gossips..");

    let received = test::gossip::messages(6, amy.local_time(), MAX_TIME_DELTA);
    for msg in received.iter().cloned() {
        amy.receive(*bob.nid(), msg);
    }

    amy.receive(
        *cid.nid(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }),
    );

    let relayed = amy.messages(*cid.nid()).collect::<BTreeSet<_>>();
    let received = received
        .into_iter()
        .chain(Some(bob.node_announcement()))
        .collect::<BTreeSet<_>>();

    assert_eq!(relayed.len(), received.len());
    assert_eq!(relayed, received);
}

#[test]
fn announcement_rebroadcast_duplicates() {
    let mut cid = Peer::cid();
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let dan = Peer::dan();
    let rids = arbitrary::set::<RepoId>(3..=3);

    amy.connect_to(&bob);
    amy.receive(*bob.nid(), cid.node_announcement());

    // These are not expected to be relayed.
    let stale = {
        let mut anns = BTreeSet::new();

        for _ in 0..5 {
            cid.elapse(LocalDuration::from_mins(1));

            anns.insert(cid.inventory_announcement());
            anns.insert(cid.node_announcement());
        }
        anns
    };

    // These are expected to be relayed.
    let expected = {
        let mut anns = BTreeSet::new();

        cid.elapse(LocalDuration::from_mins(1));
        anns.insert(cid.inventory_announcement());
        anns.insert(cid.node_announcement());
        anns.insert(bob.node_announcement());

        for rid in rids {
            amy.seed(&rid, policy::Scope::All).unwrap();
            anns.insert(cid.refs_announcement(rid));
            anns.insert(bob.refs_announcement(rid));
        }
        anns
    };

    let mut all = stale.iter().chain(expected.iter()).collect::<Vec<_>>();
    fastrand::shuffle(&mut all);

    // Amy receives all messages out of order.
    for ann in all {
        amy.receive(*bob.nid(), ann.clone());
    }

    // Amy relays just the expected ones back to Dan.
    amy.connect_from(&dan);
    amy.receive(
        *dan.nid(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::MIN,
            until: Timestamp::MAX,
        }),
    );

    let relayed = amy.messages(*dan.nid()).collect::<BTreeSet<_>>();

    assert_eq!(relayed.len(), 9);
    assert_eq!(relayed, expected);
}

#[test]
fn announcement_rebroadcast_timestamp_filtered() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let cid = Peer::cid();

    amy.connect_to(&bob);

    let delta = LocalDuration::from_mins(10);
    let first = test::gossip::messages(3, amy.local_time() - delta, LocalDuration::from_secs(0));
    let second = test::gossip::messages(3, amy.local_time(), LocalDuration::from_secs(0));
    let third = test::gossip::messages(3, amy.local_time() + delta, LocalDuration::from_secs(0));

    // Amy receives three batches of messages.
    for msg in first
        .iter()
        .chain(second.iter())
        .chain(third.iter())
        .cloned()
    {
        amy.receive(*bob.nid(), msg);
    }

    // Cid subscribes to messages within the period of the second batch only.
    amy.connect_from(&cid);
    amy.receive(
        *cid.nid(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: amy.local_time().into(),
            until: (amy.local_time() + delta).into(),
        }),
    );

    let relayed = amy.relayed(*cid.nid()).collect::<BTreeSet<_>>();
    let second = second
        .into_iter()
        .chain(Some(bob.node_announcement()))
        .collect::<BTreeSet<_>>();

    assert_eq!(relayed.len(), second.len());
    assert_eq!(relayed, second);
}

#[test]
fn announcement_relay() {
    let mut amy = Peer::amy();
    let mut bob = Peer::bob();
    let mut cid = Peer::cid();

    amy.connect_to(&bob);
    amy.connect_to(&cid);
    amy.receive(*bob.nid(), bob.inventory_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(_))
    );

    amy.receive(*bob.nid(), bob.inventory_announcement());
    assert!(
        amy.messages(*cid.nid()).next().is_none(),
        "Another inventory with the same timestamp is ignored"
    );

    bob.elapse(LocalDuration::from_mins(1));
    amy.receive(*bob.nid(), bob.inventory_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(_)),
        "Another inventory with a fresher timestamp is relayed"
    );

    amy.receive(*bob.nid(), bob.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(_)),
        "A node announcement with the same timestamp as the inventory is relayed"
    );

    amy.receive(*bob.nid(), bob.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert!(amy.messages(*cid.nid()).next().is_none(), "Only once");

    amy.receive(*cid.nid(), cid.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*bob.nid()).next(),
        Some(Message::Announcement(_)),
        "A node announcement from Cid is relayed to Bob"
    );
    assert!(
        amy.messages(*cid.nid()).next().is_none(),
        "But not back to Cid"
    );

    cid.elapse(LocalDuration::from_mins(1));
    amy.receive(*bob.nid(), cid.node_announcement())
        .elapse(service::GOSSIP_INTERVAL);
    assert!(
        amy.messages(*bob.nid()).next().is_none(),
        "Bob already know about this message, since he sent it"
    );
    assert!(
        amy.messages(*cid.nid()).next().is_none(),
        "Cid already know about this message, since she signed it"
    );
}

#[test]
fn refs_announcement_relay_public() {
    let tmp = tempfile::tempdir().unwrap();
    let mut amy = Peer::amy();
    let cid = Peer::with_storage(
        "cid",
        CID,
        Storage::open(tmp.path().join("cid"), fixtures::user()).unwrap(),
    );

    let bob = {
        const ID: u8 = BOB;
        let secret_key = SigningKey::mock(ID as usize);
        let storage = fixtures::storage(tmp.path().join("bob"), &secret_key).unwrap();
        Peer::with_storage("bob", ID, storage)
    };
    let bob_inv = bob.inventory().into_iter().collect::<Vec<_>>();

    amy.seed(&bob_inv[0], policy::Scope::All).unwrap();
    amy.seed(&bob_inv[1], policy::Scope::All).unwrap();
    amy.seed(&bob_inv[2], policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.connect_to(&cid);
    amy.receive(*cid.nid(), Message::Subscribe(Subscribe::all()));
    amy.receive(*bob.nid(), bob.refs_announcement(bob_inv[0]))
        .elapse(service::GOSSIP_INTERVAL);

    // Pretend Amy cloned Bob's repos.
    let repos = r#gen::<[MockRepository; 3]>(1);
    for (i, mut repo) in repos.into_iter().enumerate() {
        repo.doc.doc = repo
            .doc
            .doc
            .with_edits(|doc| {
                doc.visibility = Visibility::Public; // Public repos are always gossiped.
            })
            .unwrap();
        amy.storage_mut().repos.insert(bob_inv[i], repo);
    }
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(_)),
        "A refs announcement from Bob is relayed to Cid"
    );

    amy.receive(*bob.nid(), bob.refs_announcement(bob_inv[0]))
        .elapse(service::GOSSIP_INTERVAL);
    assert!(
        amy.messages(*cid.nid()).next().is_none(),
        "The same ref announcement is not relayed"
    );

    amy.receive(*bob.nid(), bob.refs_announcement(bob_inv[1]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(_)),
        "But a different one is"
    );

    amy.receive(*bob.nid(), bob.refs_announcement(bob_inv[2]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(_)),
        "And a third one is as well"
    );
}

#[test]
fn refs_announcement_relay_private() {
    let tmp = tempfile::tempdir().unwrap();
    let mut amy = Peer::amy();
    let cid = Peer::with_storage(
        "cid",
        CID,
        Storage::open(tmp.path().join("cid"), fixtures::user()).unwrap(),
    );

    let bob = {
        let signer = SigningKey::mock(BOB as usize);

        let storage = fixtures::storage(tmp.path().join("bob"), &signer).unwrap();

        Peer::with_storage("bob", BOB, storage)
    };
    let bob_inv = bob.inventory().into_iter().collect::<Vec<_>>();

    amy.seed(&bob_inv[0], policy::Scope::All).unwrap();
    amy.seed(&bob_inv[1], policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.connect_to(&cid);
    amy.receive(*cid.nid(), Message::Subscribe(Subscribe::all()));

    // The first repo is not visible to Cid.
    let repo1 = {
        let mut repo = r#gen::<MockRepository>(1);
        repo.doc.doc = repo
            .doc
            .doc
            .with_edits(|doc| {
                doc.visibility = Visibility::Private { allow: [].into() };
            })
            .unwrap();
        repo
    };
    amy.storage_mut().repos.insert(bob_inv[0], repo1);

    // The second repo is visible to Cid.
    let repo2 = {
        let mut repo = r#gen::<MockRepository>(1);
        repo.doc.doc = repo
            .doc
            .doc
            .with_edits(|doc| {
                doc.visibility = Visibility::Private {
                    allow: [(*cid.nid()).into()].into(),
                };
            })
            .unwrap();
        repo
    };
    amy.storage_mut().repos.insert(bob_inv[1], repo2);
    amy.elapse(service::GOSSIP_INTERVAL);
    amy.messages(*cid.nid()).for_each(drop);
    amy.receive(*bob.nid(), bob.refs_announcement(bob_inv[0]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        None,
        "The first ref announcement is not relayed to Cid"
    );

    amy.receive(*bob.nid(), bob.refs_announcement(bob_inv[1]))
        .elapse(service::GOSSIP_INTERVAL);
    assert_matches!(
        amy.messages(*cid.nid()).next(),
        Some(Message::Announcement(Announcement {
            message: AnnouncementMessage::Refs(_),
            ..
        })),
        "The second ref announcement is relayed to Cid"
    );
}

/// Even if Amy is not tracking Bob, Amy will fetch Bob's refs for a repo she doesn't have.
#[test]
fn refs_announcement_fetch_trusted_no_inventory() {
    let tmp = tempfile::tempdir().unwrap();
    let mut amy = Peer::with_storage(
        "amy",
        AMY,
        Storage::open(tmp.path().join("amy"), fixtures::user()).unwrap(),
    );
    let bob = {
        let mut rng = fastrand::Rng::new();
        let id = rng.u8(100..200);
        let secret_key = SigningKey::mock(id as usize);
        let storage = fixtures::storage(tmp.path().join("bob"), &secret_key).unwrap();

        Peer::with_storage("bob", id, storage)
    };
    let bob_inv = bob.inventory();
    let rid = bob_inv.iter().next().unwrap();

    amy.seed(rid, policy::Scope::Followed).unwrap();
    amy.connect_to(&bob);

    // Amy receives Bob's refs.
    amy.receive(*bob.nid(), bob.refs_announcement(*rid));

    // Amy fetches Bob's refs as this is a new repo.
    assert_matches!(amy.outbox().next(), Some(Io::Fetch { .. }));
}

/// Amy and Bob both have the same repo.
///
/// First, Amy will not fetch from Bob's `RefsAnnouncement` as Amy does not
/// track Bob as `Followed`.
///
/// Later Amy follows Bob, and will be able to fetch Bob's refs.
#[test]
fn refs_announcement_followed() {
    // Create MockStorage for Amy and Bob. Both will have repo with `rid`.
    let storage_amy = arbitrary::nonempty_storage(1);
    let rid = *storage_amy.repos.keys().next().unwrap();
    let storage_bob = storage_amy.clone();
    let mut amy = Peer::with_storage("amy", AMY, storage_amy);
    let mut bob = Peer::with_storage("bob", BOB, storage_bob);

    let node_id = *amy.nid();
    let repo = amy.storage_mut().repo_mut(&rid);
    let root = repo.identity_root().unwrap();
    let sigrefs_at = bob.signed_refs_at(root);

    repo.remotes.insert(node_id, sigrefs_at);

    // Generate some refs for Bob under their own node_id.
    let sigrefs_at = bob.signed_refs_at(root);
    let node_id = *bob.nid();
    bob.storage_mut()
        .repo_mut(&rid)
        .remotes
        .insert(node_id, sigrefs_at);

    // Amy uses Scope::Followed, and did not track Bob yet.
    amy.connect_to(&bob);
    amy.seed(&rid, policy::Scope::Followed).unwrap();

    // Amy receives Bob's refs
    amy.receive(*bob.nid(), bob.refs_announcement(rid));

    // Amy does not fetch as Amy is not tracking Bob.
    assert!(
        amy.messages(*bob.nid()).next().is_none(),
        "Amy is not tracking bob yet."
    );

    // Amy starts to track Bob.
    let (cmd, receiver) = Command::follow(*bob.nid(), Some(node::Alias::new("bob")));
    amy.command(cmd);
    let policy_change = receiver
        .recv()
        .map_err(runtime::handle::Error::from)
        .unwrap()
        .unwrap();
    assert!(policy_change);

    // Bob announces refs again.
    bob.elapse(LocalDuration::from_mins(1)); // Make sure our announcement is fresh.
    amy.receive(*bob.nid(), bob.refs_announcement(rid));
    assert_matches!(amy.outbox().next(), Some(Io::Fetch { .. }));
}

#[test]
fn refs_announcement_no_subscribe() {
    let storage = arbitrary::nonempty_storage(1);
    let rid = *storage.repos.keys().next().unwrap();
    let mut amy = Peer::with_storage("amy", AMY, storage);
    let bob = Peer::bob();
    let cid = Peer::cid();
    let id = arbitrary::r#gen(1);

    amy.seed(&id, policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.connect_to(&cid);
    amy.receive(*bob.nid(), bob.refs_announcement(rid));

    assert!(amy.messages(*cid.nid()).next().is_none());
}

#[test]
fn refs_announcement_offline() {
    let tmp = tempfile::tempdir().unwrap();
    let mut amy = {
        let id = AMY;
        let secret_key = SigningKey::mock(id as usize);
        let storage = fixtures::storage(tmp.path().join("amy"), &secret_key).unwrap();
        Peer::with_storage("amy", id, storage)
    };
    let mut bob = Peer::bob();

    amy.connect_to(&bob);
    amy.receive(*bob.nid(), Message::Subscribe(Subscribe::all()));

    let mut inv = amy.inventory();
    let rid = *inv.iter().next().unwrap();

    bob.seed(&rid, policy::Scope::All).unwrap();

    // Amy announces the refs of all projects since she hasn't announced refs for these projects
    // yet.
    for msg in amy.messages(*bob.nid()) {
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
            if node == *amy.nid() && inv.remove(&rid)
        );
    }

    // Create an issue without telling the node.
    let repo = amy.storage().repository(rid).unwrap();
    let old_refs = RefsAt::new(&repo, *amy.nid()).unwrap();
    let mut issues = radicle::issue::Cache::no_cache(&repo, amy.secret_key()).unwrap();
    issues
        .create(
            cob::Title::new("Issue while offline!").unwrap(),
            "",
            &[],
            &[],
            [],
        )
        .unwrap();
    let new_refs = RefsAt::new(&repo, *amy.nid()).unwrap();
    assert_ne!(old_refs, new_refs);

    // Now we restart Amy's node. It should pick up that something's changed in storage.
    amy.elapse(LocalDuration::from_secs(60));
    amy.database_mut()
        .addresses_mut()
        .remove(bob.nid())
        .unwrap(); // Make sure we don't reconnect automatically.
    amy.disconnected(
        *bob.nid(),
        Link::Outbound,
        &DisconnectReason::Session(session::Error::Timeout),
    );
    amy.outbox().for_each(drop);
    amy.restart();
    amy.connect_to(&bob);
    amy.receive(
        *bob.nid(),
        Message::Subscribe(Subscribe {
            filter: Filter::default(),
            since: Timestamp::from(*amy.clock()),
            until: Timestamp::MAX,
        }),
    );

    let anns = amy
        .messages(*bob.nid())
        .filter_map(|m| match m {
            Message::Announcement(Announcement {
                message: AnnouncementMessage::Refs(ann),
                ..
            }) if ann.rid == rid => Some(ann),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(anns.len(), 1);
    assert_eq!(anns.first().unwrap().rid, rid);
    assert_ne!(anns.first().unwrap().refs.first().unwrap().at, old_refs.at);
    assert_eq!(anns.first().unwrap().refs.first().unwrap().at, new_refs.at);
}

#[test]
fn inventory_relay() {
    // Topology is cid <-> amy <-> bob
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let cid = Peer::cid();
    let inv = BoundedVec::try_from(arbitrary::vec(1)).unwrap();
    let now = LocalTime::now().into();

    // Inventory from Bob relayed to Cid.
    amy.wake(); // Run all periodic tasks now so they don't trigger later.
    amy.connect_to(&bob);
    amy.connect_from(&cid);
    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now,
            },
            bob.secret_key(),
        ),
    )
    .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        amy.inventory_announcements(*cid.nid()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == *bob.nid() && timestamp == now
    );
    assert_matches!(
        amy.inventory_announcements(*bob.nid()).next(),
        None,
        "The inventory is not sent back to Bob"
    );

    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now,
            },
            bob.secret_key(),
        ),
    )
    .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        amy.inventory_announcements(*cid.nid()).next(),
        None,
        "Sending the same inventory again doesn't trigger a relay"
    );

    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv.clone(),
                timestamp: now + 1,
            },
            bob.secret_key(),
        ),
    )
    .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        amy.inventory_announcements(*cid.nid()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == *bob.nid() && timestamp == now + 1,
        "Sending a new inventory does trigger the relay"
    );

    // Inventory from Cid relayed to Bob.
    amy.receive(
        *cid.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: inv,
                timestamp: now,
            },
            cid.secret_key(),
        ),
    )
    .elapse(service::GOSSIP_INTERVAL);

    assert_matches!(
        amy.inventory_announcements(*bob.nid()).next(),
        Some(Message::Announcement(Announcement {
            node,
            message: AnnouncementMessage::Inventory(InventoryAnnouncement { timestamp, .. }),
            ..
        }))
        if node == *cid.nid() && timestamp == now
    );
}

#[test]
fn persistent_peer_reconnect_attempt() {
    use indexmap::IndexSet;

    let mut bob = Peer::bob();
    let mut cid = Peer::cid();

    let bob_connect = (*bob.nid(), bob.address()).into();
    let cid_connect = (*cid.nid(), cid.address()).into();

    let mut amy = Peer::amy_with(move |config| {
        config.config.connect = IndexSet::from_iter([bob_connect, cid_connect])
    });

    let mut sim = Simulation::new(LocalTime::now(), simulator::Options::default());

    sim.run_while([&mut amy, &mut bob, &mut cid], |s| !s.is_settled());

    let ips = amy
        .sessions()
        .connected()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert!(ips.contains(bob.nid()));
    assert!(ips.contains(cid.nid()));

    // … Negotiated …
    //
    // Now let's disconnect a peer.

    // A non-transient disconnect, such as one due to peer misbehavior will still trigger a
    // a reconnection, since this is a persistent peer.
    let reason = DisconnectReason::Session(session::Error::Misbehavior);

    for _ in 0..3 {
        amy.disconnected(*bob.nid(), Link::Outbound, &reason);
        amy.elapse(service::MAX_RECONNECTION_DELTA);
        amy.outbox()
            .find(|io| matches!(io, Io::Connect(a, _) if a == bob.nid()))
            .unwrap();

        amy.attempted(*bob.nid(), bob.address());
    }
}

#[test]
fn persistent_peer_reconnect_success() {
    use indexmap::IndexSet;

    let bob = Peer::bob();

    let bob_connect = (*bob.nid(), bob.address()).into();

    let mut amy =
        Peer::amy_with(move |config| config.config.connect = IndexSet::from_iter([bob_connect]));
    amy.connect_to(&bob);

    // A transient error such as this will cause Amy to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));
    amy.disconnected(
        *bob.nid(),
        Link::Outbound,
        &DisconnectReason::Connection(error),
    );
    amy.elapse(service::MIN_RECONNECTION_DELTA);
    amy.elapse(service::MIN_RECONNECTION_DELTA); // Trigger a second wakeup to test idempotence.

    amy.outbox()
        .find_map(|o| match o {
            Io::Connect(id, _) => Some(id),
            _ => None,
        })
        .expect("Amy attempts a re-connection");

    amy.attempted(*bob.nid(), bob.address());
    amy.connected(*bob.nid(), bob.address(), Link::Outbound);
}

#[test]
fn maintain_connections() {
    // Peers Amy starts out connected to.
    let connected = [
        Peer::new_empty_storage("connected", 0x11),
        Peer::new_empty_storage("connected", 0x12),
        Peer::new_empty_storage("connected", 0x13),
    ];
    // Peers Amy will connect to once the others disconnect.
    let mut unconnected = vec![
        Peer::new_empty_storage("unconnected", 0x21),
        Peer::new_empty_storage("unconnected", 0x22),
        Peer::new_empty_storage("unconnected", 0x23),
    ];

    let mut amy = Peer::amy();

    for peer in connected.iter() {
        amy.connect_to(peer);
    }
    assert_eq!(
        connected.len(),
        amy.sessions().len(),
        "amy should be connected to the first set of peers"
    );
    // We now import the other addresses.
    amy.import_addresses(&unconnected);

    // A non-transient error such as this will cause Amy to attempt a different peer.
    let error = session::Error::Misbehavior;
    for peer in connected.iter() {
        amy.disconnected(
            *peer.nid(),
            Link::Outbound,
            &DisconnectReason::Session(error),
        );

        let id = amy
            .outbox()
            .find_map(|o| match o {
                Io::Connect(id, _) => Some(id),
                _ => None,
            })
            .expect("Amy connects to a new peer");
        assert_ne!(id, *peer.nid());
        unconnected.retain(|p| *p.nid() != id);
    }
    assert!(
        unconnected.is_empty(),
        "Amy should connect to all unconnected peers"
    );
}

#[test]
fn maintain_connections_transient() {
    // Peers Amy starts out connected to.
    let connected = [
        Peer::new_empty_storage("connected", 0x11),
        Peer::new_empty_storage("connected", 0x12),
        Peer::new_empty_storage("connected", 0x13),
    ];
    let mut amy = Peer::amy();

    for peer in connected.iter() {
        amy.connect_to(peer);
    }
    // A transient error such as this will cause Amy to attempt a reconnection.
    let error = Arc::new(io::Error::from(io::ErrorKind::ConnectionReset));
    for peer in connected.iter() {
        amy.disconnected(
            *peer.nid(),
            Link::Outbound,
            &DisconnectReason::Connection(error.clone()),
        );
        amy.outbox()
            .find(|o| matches!(o, Io::Connect(id, _) if id == peer.nid()))
            .unwrap();
    }
}

#[test]
fn maintain_connections_failed_attempt() {
    let cid = Peer::cid();
    let mut amy = Peer::amy();
    let reason =
        DisconnectReason::Connection(Arc::new(io::Error::from(io::ErrorKind::ConnectionReset)));

    // Make sure Amy knows about Cid.
    amy.connect_to(&cid);
    amy.disconnected(*cid.nid(), Link::Outbound, &reason);
    amy.outbox()
        .find(|o| matches!(o, Io::Connect(id, _) if id == cid.nid()))
        .expect("Amy attempts Cid");
    amy.attempted(*cid.nid(), cid.address());

    // Disconnect Cid and make sure Amy doesn't try to re-connect immediately.
    amy.disconnected(*cid.nid(), Link::Outbound, &reason);
    assert_matches!(amy.outbox().find(|o| matches!(o, Io::Connect(_, _))), None);

    // Now pass some time and try again.
    amy.elapse(MAX_RECONNECTION_DELTA);
    amy.outbox()
        .find(|o| matches!(o, Io::Connect(id, _) if id == cid.nid()))
        .expect("Amy attempts Cid again");

    // Disconnect Cid and make sure Amy doesn't try to re-connect immediately.
    amy.disconnected(*cid.nid(), Link::Outbound, &reason);
    assert!(!amy.outbox().any(|o| matches!(o, Io::Connect(_, _))));
    // Or even after some short time..
    amy.elapse(MIN_RECONNECTION_DELTA);
    assert!(!amy.outbox().any(|o| matches!(o, Io::Connect(_, _))));
}

#[test]
fn maintain_connections_same_second_loop() {
    use std::io;
    use std::sync::Arc;

    let bob = Peer::bob();
    let mut amy = Peer::amy();
    let reason = DisconnectReason::Dial(Arc::new(io::Error::from(io::ErrorKind::HostUnreachable)));

    amy.connect_to(&bob);

    // Advance clock to make the connection stable.
    // This triggers `idle_connections` which sets `last_success` in the DB to the current time (T).
    amy.elapse(session::CONNECTION_STABLE_THRESHOLD);

    // Bob disconnects.
    // This triggers `maintain_connections`.
    amy.disconnected(*bob.nid(), Link::Outbound, &reason);

    let connects = amy
        .outbox()
        .filter(|o| matches!(o, Io::Connect(id, _) if id == bob.nid()))
        .count();
    assert_eq!(connects, 1, "Amy should attempt to reconnect once");

    // Simulate the dial failing instantly.
    // We DO NOT advance the clock. We just call disconnected again.
    // This triggers `maintain_connections` again.
    // Now `last_success` is T, and `last_attempt` is T.
    amy.disconnected(*bob.nid(), Link::Outbound, &reason);

    // Check if Amy tries to connect again in the exact same second.
    let immediate_retry = amy
        .outbox()
        .any(|o| matches!(o, Io::Connect(id, _) if id == *bob.nid()));

    assert!(
        !immediate_retry,
        "Amy immediately retried a connection when last_success == last_attempt"
    );
}

#[test]
fn seed_repo_subscribe() {
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let rid = arbitrary::r#gen::<RepoId>(1);

    amy.connect_to(&bob);
    let (cmd, recv) = Command::seed(rid, policy::Scope::default());
    amy.command(cmd);
    assert!(recv.recv().unwrap().unwrap());

    assert_matches!(
        amy.messages(*bob.nid()).next(),
        Some(Message::Subscribe(Subscribe {
            filter,
            since,
            ..
        })) if since == Timestamp::from(*amy.clock()) && filter.contains(&rid)
    );
}

#[test]
fn fetch_missing_inventory_on_gossip() {
    let rid = arbitrary::r#gen::<RepoId>(1);
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let now = LocalTime::now();

    amy.seed(&rid, node::policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: vec![rid].try_into().unwrap(),
                timestamp: now.into(),
            },
            bob.secret_key(),
        ),
    );
    amy.outbox()
        .find(|m| matches!(m, Io::Fetch { rid: other, .. } if other == &rid))
        .unwrap();
}

#[test]
fn fetch_missing_inventory_on_schedule() {
    let rid = arbitrary::r#gen::<RepoId>(1);
    let mut amy = Peer::amy();
    let bob = Peer::bob();
    let now = LocalTime::now();

    amy.seed(&rid, node::policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.receive(
        *bob.nid(),
        Message::inventory(
            InventoryAnnouncement {
                inventory: vec![rid].try_into().unwrap(),
                timestamp: now.into(),
            },
            bob.secret_key(),
        ),
    );
    amy.fetched(
        rid,
        *bob.nid(),
        Err(protocol::worker::FetchError::Io(
            io::ErrorKind::ConnectionReset.into(),
        )),
    );
    amy.outbox().for_each(drop);
    amy.elapse(service::SYNC_INTERVAL);
    amy.outbox()
        .find(|m| matches!(m, Io::Fetch { rid: other, .. } if other == &rid))
        .unwrap();
}

#[test]
fn queued_fetch_max_capacity() {
    let storage = arbitrary::nonempty_storage(3);
    let mut repo_keys = storage.repos.keys();
    let rid1 = *repo_keys.next().unwrap();
    let rid2 = *repo_keys.next().unwrap();
    let rid3 = *repo_keys.next().unwrap();
    let doc = storage.repos.get(&rid1).unwrap().doc.clone();
    let mut amy = Peer::with_storage("amy", AMY, storage);
    let bob = Peer::bob();

    amy.connect_to(&bob);

    // Send the first fetch.
    let (cmd, _recv1) = Command::fetch(rid1, *bob.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);

    // Send the 2nd fetch that will be queued.
    let (cmd, _recv2) = Command::fetch(rid2, *bob.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);

    // Send the 3rd fetch that will be queued.
    let (cmd, _recv3) = Command::fetch(rid3, *bob.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);

    // The first fetch is initiated.
    assert_matches!(amy.fetches().next(), Some((rid, _)) if rid == rid1);
    // We shouldn't send out the 2nd, 3rd fetch while we're doing the 1st fetch.
    assert_matches!(amy.outbox().next(), None);

    // Have enough time pass that Amy sends a "ping" to Bob.
    amy.elapse(KEEP_ALIVE_DELTA);

    // Finish the 1st fetch.
    amy.fetched(rid1, *bob.nid(), Ok(FetchResult::new(doc.clone())));

    // Now the 1st fetch is done, the 2nd fetch is dequeued.
    assert_eq!(amy.fetches().next(), Some((rid2, *bob.nid())));
    // … but not the third.
    assert_matches!(amy.fetches().next(), None);

    // Finish the 2nd fetch.
    amy.fetched(rid2, *bob.nid(), Ok(FetchResult::new(doc)));
    // Now the 2nd fetch is done, the 3rd fetch is dequeued.
    assert_eq!(amy.fetches().next(), Some((rid3, *bob.nid())));
}

// Reproduces the orphaned-fetch failure mode: a fetch is started, but on
// disconnect its result is never delivered and the disconnect skips `cancel`,
// so the `active[rid]` entry is never cleared. The repo can then no longer be
// fetched from any node even though the node keeps running. See
// `wire::Wire::worker_result` (discards the result when the peer isn't
// `Connected`) and `Service::disconnected` (skips `cancel` on a link mismatch).
#[test]
fn orphaned_fetch_blocks_repo_from_all_nodes() {
    let storage = arbitrary::nonempty_storage(1);
    let rid = *storage.repos.keys().next().unwrap();
    let doc = storage.repos.get(&rid).unwrap().doc.clone();
    let mut amy = Peer::with_storage("amy", AMY, storage);
    let bob = Peer::bob();
    let cid = Peer::cid();

    // Amy dials Bob (outbound) and starts fetching the repo, occupying
    // `active[rid]`.
    amy.connect_to(&bob);
    let (cmd, _recv) = Command::fetch(rid, *bob.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);
    assert_matches!(amy.fetches().next(), Some((r, n)) if r == rid && n == *bob.nid());

    // Bob dials Amy (inbound) while the outbound session is still up: a
    // connection conflict. The service overwrites the session's link to inbound.
    amy.connect_from(&bob);

    // The outbound transport, the one the fetch is running over, drops. Because
    // the session's link is now inbound, `Service::disconnected` early-returns
    // without cancelling the fetch, so `active[rid]` survives.
    amy.disconnected(
        *bob.nid(),
        Link::Outbound,
        &DisconnectReason::Session(session::Error::Timeout),
    );

    // Meanwhile the fetch result is never delivered (the `worker_result` discard).
    // The entry is now orphaned: nothing will ever clear it.
    assert!(
        amy.fetcher().active_fetches().contains_key(&rid),
        "active fetch should be orphaned"
    );

    // A different seed offers the same repo. It must not be fetched: the
    // orphaned entry blocks the repo from every node.
    amy.connect_to(&cid);
    let (cmd, _recv) = Command::fetch(rid, *cid.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);
    assert_matches!(amy.fetches().next(), None);

    // Delivering the missing result (what the fix guarantees) clears the entry
    // and the queued fetch from the other node proceeds.
    amy.fetched(rid, *bob.nid(), Ok(FetchResult::new(doc)));
    assert_eq!(amy.fetches().next(), Some((rid, *cid.nid())));
}

#[test]
fn queued_fetch_from_ann_same_rid() {
    let storage = arbitrary::nonempty_storage(1); // We're testing both public and private repos.
    let mut repo_keys = storage.repos.keys();
    let rid = *repo_keys.next().unwrap();

    let mut amy = Peer::with_storage("amy", AMY, storage);
    let bob = Peer::bob();
    let cid = Peer::cid();
    let dan = Peer::dan();

    let oid = arbitrary::oid();
    let ann = RefsAnnouncement {
        rid,
        refs: vec![RefsAt {
            remote: *cid.nid(),
            at: oid,
        }]
        .try_into()
        .unwrap(),
        timestamp: Timestamp::from(*bob.clock()),
    };

    amy.seed(&rid, policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.connect_to(&dan);
    amy.connect_to(&cid);

    // Send the first announcement.
    amy.receive(*bob.nid(), bob.announcement(ann.clone()));
    // Send the 2nd announcement that will be queued.
    amy.receive(*dan.nid(), dan.announcement(ann.clone()));
    // Send the 3rd announcement that will be queued.
    amy.receive(*cid.nid(), cid.announcement(ann));

    // The first fetch is initiated.
    assert_matches!(amy.fetches().next(), Some((rid_, nid_)) if rid_ == rid && nid_ == *bob.nid());
    // We shouldn't send out the 2nd, 3rd fetch while we're doing the 1st fetch.
    assert_matches!(amy.fetches().next(), None);

    // Have enough time pass that Amy sends a "ping" to Bob.
    amy.elapse(KEEP_ALIVE_DELTA);

    let refname = cid
        .nid()
        .to_namespace()
        .join(git::fmt::refname!("refs/sigrefs"));

    // Finish the 1st fetch.
    // Ensure the ref is in the storage and cache.
    let repo = amy.storage_mut().repo_mut(&rid);
    let sigrefs_at = cid.signed_refs_at(repo.identity_root().unwrap());
    repo.remotes.insert(*cid.nid(), sigrefs_at);
    amy.database_mut()
        .refs_mut()
        .set(&rid, cid.nid(), &SIGREFS_BRANCH, oid, LocalTime::now())
        .unwrap();
    amy.fetched(
        rid,
        *bob.nid(),
        Ok(FetchResult {
            updated: vec![RefUpdate::Created {
                name: refname.clone(),
                oid,
            }],
            canonical: protocol::worker::fetch::UpdatedCanonicalRefs::default(),
            namespaces: [*cid.nid()].into_iter().collect(),
            clone: false,
            doc: arbitrary::r#gen(1),
        }),
    );
    // Now the 1st fetch is done, but the 2nd and 3rd fetches are redundant.
    assert_matches!(amy.fetches().next(), None);
}

#[test]
fn queued_fetch_from_command_same_rid() {
    let storage = arbitrary::nonempty_storage(3);
    let mut repo_keys = storage.repos.keys();
    let rid1 = *repo_keys.next().unwrap();

    let mut amy = Peer::with_storage("amy", AMY, storage);
    let bob = Peer::bob();
    let cid = Peer::cid();
    let dan = Peer::dan();

    amy.connect_to(&bob);
    amy.connect_to(&dan);
    amy.connect_to(&cid);

    // Send the first fetch.
    let (cmd, _recv1) = Command::fetch(rid1, *bob.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);

    // Send the 2nd fetch that will be queued.
    let (cmd, _recv2) = Command::fetch(rid1, *dan.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);

    // Send the 3rd fetch that will be queued.
    let (cmd, _recv3) = Command::fetch(rid1, *cid.nid(), DEFAULT_TIMEOUT, None);
    amy.command(cmd);

    // Peers Amy will fetch from.
    let mut peers = [*bob.nid(), *dan.nid(), *cid.nid()]
        .into_iter()
        .collect::<BTreeSet<_>>();

    // The first fetch is initiated.
    let (rid, nid) = amy.fetches().next().unwrap();
    assert_eq!(rid, rid1);
    assert!(peers.remove(&nid));

    // We shouldn't send out the 2nd, 3rd fetch while we're doing the 1st fetch.
    assert_matches!(amy.outbox().next(), None);

    // Have enough time pass that Amy sends a "ping" to Bob.
    amy.elapse(KEEP_ALIVE_DELTA);

    // Finish the 1st fetch.
    amy.fetched(rid1, nid, Ok(arbitrary::r#gen::<FetchResult>(1)));
    // Now the 1st fetch is done, the 2nd fetch is dequeued.
    let (rid, nid) = amy.fetches().next().unwrap();
    assert_eq!(rid, rid1);
    assert!(peers.remove(&nid));

    // … but not the third.
    assert_matches!(amy.fetches().next(), None);

    // Finish the 2nd fetch.
    amy.fetched(rid1, nid, Ok(arbitrary::r#gen::<FetchResult>(1)));
    // Now the 2nd fetch is done, the 3rd fetch is dequeued.
    assert_matches!(amy.fetches().next(), Some((rid, nid)) if rid == rid1 && peers.remove(&nid));
    // All fetches were initiated.
    assert!(peers.is_empty());
}

#[test]
fn refs_synced_event() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path(), fixtures::user()).unwrap();

    let mut amy = Peer::with_storage("amy", AMY, storage.clone());
    let bob = Peer::bob();
    let cid = Peer::with_storage("cid", CID, storage);

    let acme = amy.project("acme", "");
    let events = amy.events();
    let ann = AnnouncementMessage::from(RefsAnnouncement {
        rid: acme,
        refs: vec![RefsAt::new(&amy.storage().repository(acme).unwrap(), *amy.nid()).unwrap()]
            .try_into()
            .unwrap(),
        timestamp: Timestamp::from(*bob.clock()),
    });
    let msg = ann.signed(bob.secret_key());

    amy.seed(&acme, policy::Scope::All).unwrap();
    amy.connect_to(&bob);
    amy.receive(*bob.nid(), Message::Announcement(msg));

    events
        .wait(
            |e| {
                matches!(
                    e,
                    Event::RefsSynced { remote, rid, .. }
                    if rid == &acme && remote == bob.nid()
                )
                .then_some(())
            },
            time::Duration::from_secs(3),
        )
        .unwrap();

    // Now a relayed announcement.
    amy.receive(*bob.nid(), cid.node_announcement());
    amy.receive(*bob.nid(), cid.refs_announcement(acme));

    events
        .wait(
            |e| matches!(e, Event::RefsSynced { remote, .. } if remote == cid.nid()).then_some(()),
            time::Duration::from_secs(3),
        )
        .unwrap();
}

#[test]
fn init_and_seed() {
    let tempdir = tempfile::tempdir().unwrap();

    let storage_amy =
        Storage::open(tempdir.path().join("amy").join("storage"), fixtures::user()).unwrap();
    let (repo, _) = fixtures::repository(tempdir.path().join("working"));
    let mut amy = Peer::with_storage("amy", AMY, storage_amy);

    let storage_bob =
        Storage::open(tempdir.path().join("bob").join("storage"), fixtures::user()).unwrap();
    let mut bob = Peer::with_storage("bob", BOB, storage_bob);

    let storage_cid =
        Storage::open(tempdir.path().join("cid").join("storage"), fixtures::user()).unwrap();
    let mut cid = Peer::with_storage("cid", CID, storage_cid);

    remote::mock::register(amy.nid(), amy.storage().path());
    remote::mock::register(cid.nid(), cid.storage().path());
    remote::mock::register(bob.nid(), bob.storage().path());
    local::register(amy.storage().clone());

    // Amy and Bob connect to Cid.
    amy.command(service::Command::Connect(
        *cid.nid(),
        cid.address(),
        ConnectOptions::default(),
    ));
    bob.command(service::Command::Connect(
        *cid.nid(),
        cid.address(),
        ConnectOptions::default(),
    ));

    // Amy creates a new project.
    let (proj_id, _, _) = rad::init(
        &repo,
        "amy".try_into().unwrap(),
        "amy's repo",
        git::fmt::refname!("master"),
        Visibility::default(),
        amy.secret_key(),
        amy.storage(),
    )
    .unwrap();

    let mut sim = Simulation::new(LocalTime::now(), simulator::Options::default());

    let bob_events = bob.events();

    // Neither Eve nor Bob have Amy's project for now.
    assert!(cid.get(proj_id).unwrap().is_none());
    assert!(bob.get(proj_id).unwrap().is_none());

    // Bob seeds Amy's project.
    let (cmd, receiver) = service::Command::seed(proj_id, policy::Scope::default());
    bob.command(cmd);
    assert!(receiver.recv().unwrap().unwrap());

    // Cid seeds Amy's project.
    let (cmd, receiver) = service::Command::seed(proj_id, policy::Scope::default());
    cid.command(cmd);
    assert!(receiver.recv().unwrap().unwrap());

    // We now expect Cid to fetch Amy's project from Amy.
    // Then we expect Bob to fetch Amy's project from Cid.
    amy.elapse(LocalDuration::from_secs(1)); // Make sure our announcement is fresh.
    let (cmd, _) = service::Command::add_inventory(proj_id);
    amy.command(cmd);

    sim.run_while([&mut amy, &mut bob, &mut cid], |s| !s.is_settled());

    log::debug!(target: "test", "Simulation is over");

    // TODO: Refs should be compared between the two peers.

    log::debug!(target: "test", "Waiting for {} to fetch {} from {}..", *bob.nid(), proj_id,*cid.nid());
    bob_events
        .iter()
        .find(|e| {
            matches!(
                e,
                radicle::node::events::Event::RefsFetched { remote, .. }
                if *remote == *cid.nid()
            )
        })
        .expect("Bob fetched from Cid");

    assert!(cid.storage().get(proj_id).unwrap().is_some());
    assert!(bob.storage().get(proj_id).unwrap().is_some());
}

#[test]
fn prop_inventory_exchange_dense() {
    fn property(amy_inv: MockStorage, bob_inv: MockStorage, cid_inv: MockStorage) {
        let rng = fastrand::Rng::new();
        let amy = Peer::with_storage(
            "amy",
            AMY,
            amy_inv
                .clone()
                .map(|doc| doc.visibility = Visibility::Public),
        );
        let mut bob = Peer::with_storage(
            "bob",
            BOB,
            bob_inv
                .clone()
                .map(|doc| doc.visibility = Visibility::Public),
        );
        let mut cid = Peer::with_storage(
            "cid",
            CID,
            cid_inv
                .clone()
                .map(|doc| doc.visibility = Visibility::Public),
        );
        let mut routing = RandomMap::with_hasher(rng.clone().into());

        for (inv, peer) in &[
            (amy_inv.repos, *amy.nid()),
            (bob_inv.repos, *bob.nid()),
            (cid_inv.repos, *cid.nid()),
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
            *amy.nid(),
            amy.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            *cid.nid(),
            cid.address(),
            ConnectOptions::default(),
        ));
        cid.command(Command::Connect(
            *amy.nid(),
            amy.address(),
            ConnectOptions::default(),
        ));

        let mut peers: RandomMap<_, _> = [(*amy.nid(), amy), (*bob.nid(), bob), (*cid.nid(), cid)]
            .into_iter()
            .collect();
        let mut simulator = Simulation::new(LocalTime::now(), simulator::Options::default());

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
        .r#gen(qcheck::Gen::new(5))
        .tests(20)
        .quickcheck(property as fn(MockStorage, MockStorage, MockStorage));
}

#[test]
fn announcement_message_amplification() {
    let mut results = Vec::new();
    let mut rng = fastrand::Rng::new();

    while results.len() < *TEST_CASES {
        let mut amy = Peer::amy();
        let mut bob = Peer::bob();
        let mut cid = Peer::cid();
        let mut dan = Peer::dan();
        let mut eve = Peer::eve();
        let mut sim = Simulation::new(
            LocalTime::now(),
            simulator::Options {
                latency: 0..1, // 0 - 1s
                failure_rate: 0.,
            },
        );
        let rid = r#gen::<RepoId>(1);

        // Make sure the node gossip intervals are not accidentally synchronized.
        amy.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        bob.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        cid.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        dan.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));
        eve.elapse(LocalDuration::from_millis(
            rng.u128(0..=service::GOSSIP_INTERVAL.as_millis()),
        ));

        // Fully-connected network.
        amy.command(Command::Connect(
            *bob.nid(),
            bob.address(),
            ConnectOptions::default(),
        ));
        amy.command(Command::Connect(
            *cid.nid(),
            cid.address(),
            ConnectOptions::default(),
        ));
        amy.command(Command::Connect(
            *dan.nid(),
            dan.address(),
            ConnectOptions::default(),
        ));
        amy.command(Command::Connect(
            *eve.nid(),
            eve.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            *cid.nid(),
            cid.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            *dan.nid(),
            dan.address(),
            ConnectOptions::default(),
        ));
        bob.command(Command::Connect(
            *eve.nid(),
            eve.address(),
            ConnectOptions::default(),
        ));
        cid.command(Command::Connect(
            *dan.nid(),
            dan.address(),
            ConnectOptions::default(),
        ));
        cid.command(Command::Connect(
            *eve.nid(),
            eve.address(),
            ConnectOptions::default(),
        ));
        dan.command(Command::Connect(
            *eve.nid(),
            eve.address(),
            ConnectOptions::default(),
        ));

        // Let the nodes connect to each other.
        sim.run_while([&mut amy, &mut bob, &mut cid, &mut dan, &mut eve], |s| {
            s.elapsed() < LocalDuration::from_mins(3)
        });

        // Ensure nodes are all connected; otherwise, skip this test run.
        if amy.sessions().connected().count() != 4 {
            continue;
        }
        if bob.sessions().connected().count() != 4 {
            continue;
        }
        if cid.sessions().connected().count() != 4 {
            continue;
        }
        if dan.sessions().connected().count() != 4 {
            continue;
        }
        if eve.sessions().connected().count() != 4 {
            continue;
        }

        let timestamp = (*amy.clock()).into();
        amy.storage_mut()
            .repos
            .insert(rid, r#gen::<MockRepository>(1));
        let (cmd, _) = Command::add_inventory(rid);
        amy.command(cmd);

        sim.run_while([&mut amy, &mut bob, &mut cid, &mut dan, &mut eve], |s| {
            s.elapsed() < LocalDuration::from_mins(3)
        });

        // Make sure they have the routing table entry.
        for node in [&bob, &cid, &dan, &eve] {
            assert!(
                node.database()
                    .routing()
                    .get(&rid)
                    .unwrap()
                    .contains(amy.nid())
            );
        }

        // Count how many copies of Amy's inventory message have been received by peers.
        let received = sim.messages().iter().filter(|m| {
            matches!(
                m,
                (_, _, Message::Announcement(Announcement {
                    node,
                    message: AnnouncementMessage::Inventory(i),
                    ..
                }))
                if node == amy.nid() && i.inventory.to_vec() == vec![rid] && i.timestamp == timestamp
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
