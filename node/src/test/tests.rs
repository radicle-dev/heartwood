use std::io;
use std::sync::Arc;

use nakamoto_net as nakamoto;
use nakamoto_net::simulator;
use nakamoto_net::simulator::{Peer as _, Simulation};
use nakamoto_net::Protocol as _;

use crate::collections::{HashMap, HashSet};
use crate::protocol::*;
use crate::storage::ReadStorage;
#[allow(unused)]
use crate::test::logger;
use crate::test::peer::Peer;
use crate::test::storage::MockStorage;
use crate::*;

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

    alice.connect_to(&bob.addr());
    alice.connect_to(&eve.addr());

    let peers = alice
        .protocol
        .peers()
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

    alice.connect_from(&bob.addr());
    alice.connect_from(&eve.addr());

    let peers = alice
        .protocol
        .peers()
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
        connect: vec![bob.addr(), eve.addr()],
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
fn test_inventory_relay_bad_seq() {
    let mut alice = Peer::new("alice", [7, 7, 7, 7], MockStorage::empty());
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());

    alice.connect_to(&bob.addr());
    alice.receive(
        &bob.addr(),
        Message::Inventory {
            seq: 0,
            inv: vec![],
            origin: None,
        },
    );
    assert_matches!(
        alice.outbox().next(),
        Some(Io::Disconnect(addr, DisconnectReason::Error(PeerError::InvalidSequenceNumber(seq))))
        if addr == bob.addr() && seq == 0
    );
}

#[test]
fn test_inventory_relay() {
    // Topology is eve <-> alice <-> bob
    let mut alice = Peer::new("alice", [7, 7, 7, 7], MockStorage::empty());
    let bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());
    let inv = vec![];

    // Inventory from Bob relayed to Eve.
    alice.connect_to(&bob.addr());
    alice.connect_from(&eve.addr());
    alice.receive(
        &bob.addr(),
        Message::Inventory {
            seq: 1,
            inv: inv.clone(),
            origin: None,
        },
    );
    assert_matches!(
        alice.messages(&eve.addr()).next(),
        Some(Message::Inventory { seq, origin, .. })
        if origin == Some(bob.ip) && seq == 1
    );
    assert_matches!(
        alice.messages(&bob.addr()).next(),
        None,
        "The inventory is not sent back to Bob"
    );

    alice.receive(
        &bob.addr(),
        Message::Inventory {
            seq: 1,
            inv: inv.clone(),
            origin: None,
        },
    );
    assert_matches!(
        alice.messages(&eve.addr()).next(),
        None,
        "Sending the same inventory again doesn't trigger a relay"
    );

    alice.receive(
        &bob.addr(),
        Message::Inventory {
            seq: 2,
            inv: inv.clone(),
            origin: None,
        },
    );
    assert_matches!(
        alice.messages(&eve.addr()).next(),
        Some(Message::Inventory { seq, origin, .. })
        if origin == Some(bob.ip) && seq == 2,
        "Sending a new inventory does trigger the relay"
    );

    // Inventory from Eve relayed to Bob.
    alice.receive(
        &eve.addr(),
        Message::Inventory {
            seq: 4,
            inv,
            origin: None,
        },
    );
    assert_matches!(
        alice.messages(&bob.addr()).next(),
        Some(Message::Inventory { seq, origin, .. })
        if origin == Some(eve.ip) && seq == 4
    );
}

#[test]
fn test_persistent_peer_reconnect() {
    let mut bob = Peer::new("bob", [8, 8, 8, 8], MockStorage::empty());
    let mut eve = Peer::new("eve", [9, 9, 9, 9], MockStorage::empty());
    let mut alice = Peer::config(
        "alice",
        Config {
            connect: vec![bob.addr(), eve.addr()],
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
        .peers()
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
        nakamoto::DisconnectReason::DialError(error.clone()),
    );
    assert_matches!(alice.outbox().next(), None);

    for _ in 0..MAX_CONNECTION_ATTEMPTS {
        alice.disconnected(
            &bob.addr(),
            nakamoto::DisconnectReason::ConnectionError(error.clone()),
        );
        assert_matches!(alice.outbox().next(), Some(Io::Connect(a)) if a == bob.addr());
        assert_matches!(alice.outbox().next(), None);

        alice.attempted(&bob.addr());
    }

    // After the max connection attempts, a disconnect doesn't trigger a reconnect.
    alice.disconnected(
        &bob.addr(),
        nakamoto::DisconnectReason::ConnectionError(error),
    );
    assert_matches!(alice.outbox().next(), None);
}

#[test]
fn prop_inventory_exchange_dense() {
    fn property(alice_inv: MockStorage, bob_inv: MockStorage, eve_inv: MockStorage) {
        let rng = fastrand::Rng::new();
        let alice = Peer::new("alice", [7, 7, 7, 7], alice_inv.clone());
        let mut bob = Peer::new("bob", [8, 8, 8, 8], bob_inv.clone());
        let mut eve = Peer::new("eve", [9, 9, 9, 9], eve_inv.clone());
        let mut routing = Routing::with_hasher(rng.clone().into());

        for (inv, peer) in &[
            (alice_inv.inventory, alice.addr().ip()),
            (bob_inv.inventory, bob.addr().ip()),
            (eve_inv.inventory, eve.addr().ip()),
        ] {
            for (proj, _) in inv {
                routing
                    .entry(proj.clone())
                    .or_insert_with(|| HashSet::with_hasher(rng.clone().into()))
                    .insert(*peer);
            }
        }

        // Fully-connected.
        bob.command(Command::Connect(alice.addr()));
        bob.command(Command::Connect(eve.addr()));
        eve.command(Command::Connect(alice.addr()));
        eve.command(Command::Connect(bob.addr()));

        let mut peers: HashMap<_, _> = [(alice.ip, alice), (bob.ip, bob), (eve.ip, eve)]
            .into_iter()
            .collect();
        let mut simulator = Simulation::new(LocalTime::now(), rng, simulator::Options::default())
            .initialize(peers.values_mut());

        simulator.run_while(peers.values_mut(), |s| !s.is_settled());

        for (proj_id, remotes) in &routing {
            for peer in peers.values() {
                let lookup = peer.lookup(proj_id);

                if lookup.local.is_some() {
                    peer.storage()
                        .get(proj_id)
                        .expect("There are no errors querying storage")
                        .expect("The project is available locally");
                } else {
                    for remote in &lookup.remote {
                        peers[remote]
                            .storage()
                            .get(proj_id)
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
