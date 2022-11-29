use std::{env, net, process, thread};

use anyhow::Context as _;
use cyphernet::addr::PeerAddr;
use nakamoto_net::{LocalDuration, LocalTime};
use reactor::poller::popol;
use reactor::Reactor;

use radicle::profile;
use radicle_node::client::handle::Handle;
use radicle_node::client::{ADDRESS_DB_FILE, NODE_DIR, ROUTING_DB_FILE, TRACKING_DB_FILE};
use radicle_node::crypto::ssh::keystore::MemorySigner;
use radicle_node::prelude::{Address, NodeId};
use radicle_node::service::{routing, tracking};
use radicle_node::wire::Transport;
use radicle_node::{address, control, logger, service};

#[derive(Debug)]
struct Options {
    connect: Vec<(NodeId, Address)>,
    external_addresses: Vec<Address>,
    limits: service::config::Limits,
    // FIXME(cloudhead): Listen on incoming connections.
    #[allow(dead_code)]
    listen: Vec<net::SocketAddr>,
}

impl Options {
    fn from_env() -> Result<Self, anyhow::Error> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut connect = Vec::new();
        let mut external_addresses = Vec::new();
        let mut limits = service::config::Limits::default();
        let mut listen = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("connect") => {
                    let peer: PeerAddr<NodeId, Address> = parser.value()?.parse()?;
                    connect.push((*peer.id(), peer.addr().clone()));
                }
                Long("external-address") => {
                    let addr = parser.value()?.parse()?;
                    external_addresses.push(addr);
                }
                Long("limit-routing-max-age") => {
                    let secs: u64 = parser.value()?.parse()?;
                    limits.routing_max_age = LocalDuration::from_secs(secs);
                }
                Long("limit-routing-max-size") => {
                    limits.routing_max_size = parser.value()?.parse()?;
                }
                Long("listen") => {
                    let addr = parser.value()?.parse()?;
                    listen.push(addr);
                }
                Long("help") => {
                    println!("usage: radicle-node [--connect <addr>]..");
                    process::exit(0);
                }
                _ => anyhow::bail!(arg.unexpected()),
            }
        }

        if external_addresses.len() > service::ADDRESS_LIMIT {
            anyhow::bail!(
                "external address limit ({}) exceeded",
                service::ADDRESS_LIMIT,
            )
        }

        Ok(Self {
            connect,
            external_addresses,
            limits,
            listen,
        })
    }
}

fn main() -> anyhow::Result<()> {
    logger::init(log::Level::Debug)?;

    let options = Options::from_env()?;
    let profile = radicle::Profile::load().context("Failed to load node profile")?;
    let node = profile.node();
    let passphrase = env::var(profile::env::RAD_PASSPHRASE)
        .context("`RAD_PASSPHRASE` is required to be set for the node to establish connections")?
        .into();
    let signer = MemorySigner::load(&profile.keystore, passphrase)?;
    let negotiator = signer.clone();
    let config = service::Config {
        connect: options.connect.into_iter().collect(),
        external_addresses: options.external_addresses,
        limits: options.limits,
        ..service::Config::default()
    };
    let proxy_addr = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
    let network = config.network;
    let rng = fastrand::Rng::new();
    let clock = LocalTime::now();
    let storage = profile.storage;
    let node_dir = profile.home.join(NODE_DIR);
    let address_db = node_dir.join(ADDRESS_DB_FILE);
    let routing_db = node_dir.join(ROUTING_DB_FILE);
    let tracking_db = node_dir.join(TRACKING_DB_FILE);

    log::info!("Opening address book {}..", address_db.display());
    let addresses = address::Book::open(address_db)?;

    log::info!("Opening routing table {}..", routing_db.display());
    let routing = routing::Table::open(routing_db)?;

    log::info!("Opening tracking policy table {}..", tracking_db.display());
    let tracking = tracking::Config::open(tracking_db)?;

    log::info!("Initializing service ({:?})..", network);
    let service = service::Service::new(
        config, clock, routing, storage, addresses, tracking, signer, rng,
    );

    let wire = Transport::new(service, negotiator, proxy_addr, clock);
    let reactor = Reactor::new(wire, popol::Poller::new());
    let handle = Handle::from(reactor.controller());
    let control = thread::spawn(move || control::listen(node, handle));

    control.join().unwrap()?;
    reactor.join().unwrap();

    Ok(())
}
