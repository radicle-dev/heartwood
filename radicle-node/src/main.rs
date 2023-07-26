use std::io;
use std::{env, fs, net, process};

use anyhow::anyhow;
use crossbeam_channel as chan;
use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;

use radicle::node;
use radicle::prelude::Signer;
use radicle::profile;
use radicle::version;
use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::prelude::{Address, NodeId};
use radicle_node::Runtime;
use radicle_node::{logger, service, signals};

pub const NAME: &str = "radicle-node";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HEAD: &str = env!("GIT_HEAD");

pub const HELP_MSG: &str = r#"
Usage

   radicle-node [<option>...]

   If you're running a public seed node, make sure to use `--listen` to bind a listening socket to
   eg. `0.0.0.0:8776`, and `--external-address` to advertize your public/external address to the
   network.

Options

    --connect            <peer>         Connect to the given peer address on start
    --external-address   <address>      Publicly accessible address (may be specified multiple times)
    --git-daemon         <address>      Address to bind git-daemon to (default 0.0.0.0:9418)
    --tracking-policy    (track|block)  Default tracking policy
    --tracking-scope     (trusted|all)  Default scope for tracking policies
    --force                             Force start even if an existing control socket is found
    --help                              Print help
    --listen             <address>      Address to listen on
    --version                           Print program version
"#;

#[derive(Debug)]
struct Options {
    daemon: Option<net::SocketAddr>,
    listen: Vec<net::SocketAddr>,
    force: bool,
}

impl Options {
    fn from_env(config: &mut node::Config) -> Result<Self, anyhow::Error> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut listen = Vec::new();
        let mut daemon = None;
        let mut force = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("connect") => {
                    let peer: PeerAddr<NodeId, Address> = parser.value()?.parse()?;
                    config.connect.insert(peer.into());
                }
                Long("external-address") => {
                    let addr = parser.value()?.parse()?;
                    config.external_addresses.push(addr);
                }
                Long("force") => {
                    force = true;
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
                    config.policy = policy;
                }
                Long("tracking-scope") => {
                    let scope = parser
                        .value()?
                        .parse()
                        .map_err(|s| anyhow!("unknown tracking scope {:?}", s))?;
                    config.scope = scope;
                }
                Long("limit-routing-max-age") => {
                    let secs: u64 = parser.value()?.parse()?;
                    config.limits.routing_max_age = LocalDuration::from_secs(secs);
                }
                Long("limit-routing-max-size") => {
                    config.limits.routing_max_size = parser.value()?.parse()?;
                }
                Long("limit-fetch-concurrency") => {
                    config.limits.fetch_concurrency = parser.value()?.parse()?;
                }
                Long("listen") => {
                    let addr = parser.value()?.parse()?;
                    listen.push(addr);
                }
                Long("help") | Short('h') => {
                    println!("{HELP_MSG}");
                    process::exit(0);
                }
                Long("version") => {
                    version::print(&mut io::stdout(), NAME, VERSION, GIT_HEAD)?;
                    process::exit(0);
                }
                _ => anyhow::bail!(arg.unexpected()),
            }
        }

        if config.external_addresses.len() > service::ADDRESS_LIMIT {
            anyhow::bail!(
                "external address limit ({}) exceeded",
                service::ADDRESS_LIMIT,
            )
        }

        Ok(Self {
            daemon,
            force,
            listen,
        })
    }
}

fn execute() -> anyhow::Result<()> {
    logger::init(log::Level::Debug)?;

    log::info!(target: "node", "Starting node..");
    log::info!(target: "node", "Version {} ({})", env!("CARGO_PKG_VERSION"), env!("GIT_HEAD"));

    let home = profile::home()?;
    let mut config = profile::Config::load(&home.config())?.node;

    log::info!(target: "node", "Unlocking node keystore..");

    let passphrase = profile::env::passphrase();
    let keystore = Keystore::new(&home.keys());
    let signer = MemorySigner::load(&keystore, passphrase)?;

    log::info!(target: "node", "Node ID is {}", signer.public_key());

    let options = Options::from_env(&mut config)?;
    let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
    let daemon = options.daemon.unwrap_or_else(|| {
        net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), radicle::git::PROTOCOL_PORT)
    });

    let (notify, signals) = chan::bounded(1);
    signals::install(notify)?;

    if options.force {
        log::debug!(target: "node", "Removing existing control socket..");
        fs::remove_file(home.socket()).ok();
    }
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
