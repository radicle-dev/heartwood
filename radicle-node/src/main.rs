use std::{env, net, process, thread};

use anyhow::Context as _;

use nakamoto_net::LocalDuration;

use radicle::profile;
use radicle_node::crypto::ssh::keystore::MemorySigner;
use radicle_node::logger;
use radicle_node::prelude::Address;
use radicle_node::{client, control, service};

type Reactor = nakamoto_net_poll::Reactor<net::TcpStream>;

#[derive(Debug)]
struct Options {
    connect: Vec<Address>,
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
                    let addr = parser.value()?.parse()?;
                    connect.push(addr);
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
    let client = client::Client::<Reactor>::new().context("Failed to initialize client")?;
    let signer = match profile.signer() {
        Ok(signer) => signer.boxed(),
        Err(err) => {
            let passphrase = env::var(profile::env::RAD_PASSPHRASE)
                .context("Either ssh-agent must be initialized, or `RAD_PASSPHRASE` must be set")
                .context(err)?
                .into();
            MemorySigner::load(&profile.keystore, passphrase)?.boxed()
        }
    };
    let handle = client.handle();
    let config = client::Config {
        service: service::Config {
            connect: options.connect,
            external_addresses: options.external_addresses,
            limits: options.limits,
            ..service::Config::default()
        },
        listen: options.listen,
    };

    let t1 = thread::spawn(move || control::listen(node, handle));
    let t2 = thread::spawn(move || client.run(config, profile, signer));

    t1.join().unwrap()?;
    t2.join().unwrap()?;

    Ok(())
}
