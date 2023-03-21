use std::{net, process};

use anyhow::{anyhow, Context as _};
use crossbeam_channel as chan;
use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;

use radicle::prelude::Signer;
use radicle::profile;
use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::prelude::{Address, NodeId};
use radicle_node::service::tracking::{Policy, Scope};
use radicle_node::Runtime;
use radicle_node::{logger, service, signals};
use radicle_term as term;

pub const HELP_MSG: &str = r#"
Usage

   radicle-node [<option>...]

Options

    --connect          <peer>        Connect to the given peer address on start
    --external-address <address>     Publicly accessible address (default 0.0.0.0:8776)
    --git-daemon       <address>     Address to bind git-daemon to (default 0.0.0.0:9418)
    --tracking-policy  (track|block) Default tracking policy
    --tracking-scope   (trusted|all) Default scope for tracking policies
    --help                           Print help
    --listen           <address>     Address to listen on

"#;

#[derive(Debug)]
struct Options {
    connect: Vec<(NodeId, Address)>,
    external_addresses: Vec<Address>,
    daemon: Option<net::SocketAddr>,
    limits: service::config::Limits,
    listen: Vec<net::SocketAddr>,
    tracking_policy: Policy,
    tracking_scope: Scope,
}

impl Options {
    fn from_env() -> Result<Self, anyhow::Error> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut connect = Vec::new();
        let mut external_addresses = Vec::new();
        let mut limits = service::config::Limits::default();
        let mut listen = Vec::new();
        let mut daemon = None;
        let mut tracking_policy = Policy::default();
        let mut tracking_scope = Scope::default();

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
                Long("git-daemon") => {
                    let addr = parser.value()?.parse()?;
                    daemon = Some(addr);
                }
                Long("tracking-policy") => {
                    let policy = parser
                        .value()?
                        .parse()
                        .map_err(|s| anyhow!("unknown tracking policy {:?}", s))?;
                    tracking_policy = policy;
                }
                Long("tracking-scope") => {
                    let scope = parser
                        .value()?
                        .parse()
                        .map_err(|s| anyhow!("unknown tracking scope {:?}", s))?;
                    tracking_scope = scope;
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
                    println!("{HELP_MSG}");
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
            daemon,
            external_addresses,
            limits,
            listen,
            tracking_policy,
            tracking_scope,
        })
    }
}

fn execute() -> anyhow::Result<()> {
    logger::init(log::Level::Debug)?;

    log::info!(target: "node", "Starting node..");
    log::info!(target: "node", "Version {} ({})", env!("CARGO_PKG_VERSION"), env!("GIT_HEAD"));

    let options = Options::from_env()?;
    let home = profile::home()?;

    log::info!(target: "node", "Unlocking node keystore..");

    let passphrase = term::io::passphrase(profile::env::RAD_PASSPHRASE)
        .context(format!("`{}` must be set", profile::env::RAD_PASSPHRASE))?;
    let keystore = Keystore::new(&home.keys());
    let signer = MemorySigner::load(&keystore, passphrase)?;

    log::info!(target: "node", "Node ID is {}", signer.public_key());

    let config = service::Config {
        connect: options.connect.into_iter().collect(),
        external_addresses: options.external_addresses,
        limits: options.limits,
        policy: options.tracking_policy,
        scope: options.tracking_scope,
        ..service::Config::default()
    };
    let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
    let daemon = options.daemon.unwrap_or_else(|| {
        net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), radicle::git::PROTOCOL_PORT)
    });

    let (notify, signals) = chan::bounded(1);
    signals::install(notify)?;

    Runtime::init(home, config, options.listen, proxy, daemon, signals, signer)?.run()?;

    Ok(())
}

fn main() {
    if let Err(err) = execute() {
        if let Some(src) = err.source() {
            log::error!(target: "node", "Fatal: {err}: {src}");
        } else {
            log::error!(target: "node", "Fatal: {err}");
        }
        process::exit(1);
    }
}
