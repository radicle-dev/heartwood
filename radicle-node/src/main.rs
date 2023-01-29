use std::{env, net, process};

use anyhow::Context as _;
use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;

use radicle::profile;
use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::prelude::{Address, NodeId};
use radicle_node::Runtime;
use radicle_node::{logger, service};

#[derive(Debug)]
struct Options {
    connect: Vec<(NodeId, Address)>,
    external_addresses: Vec<Address>,
    limits: service::config::Limits,
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
                    connect.push((peer.id, peer.addr.clone()));
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

fn execute() -> anyhow::Result<()> {
    logger::init(log::Level::Debug)?;

    let options = Options::from_env()?;
    let home = profile::home()?;
    let passphrase = env::var(profile::env::RAD_PASSPHRASE)
        .context("`RAD_PASSPHRASE` is required to be set for the node to establish connections")?
        .into();
    let keystore = Keystore::new(&home.keys());
    let signer = MemorySigner::load(&keystore, passphrase)?;
    let config = service::Config {
        connect: options.connect.into_iter().collect(),
        external_addresses: options.external_addresses,
        limits: options.limits,
        ..service::Config::default()
    };
    let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
    let daemon = ([0, 0, 0, 0], 9418).into();

    Runtime::init(home, config, options.listen, proxy, daemon, signer)?.run()?;

    Ok(())
}

fn main() {
    if let Err(err) = execute() {
        log::error!(target: "node", "Fatal: {}", err);
        process::exit(1);
    }
}
