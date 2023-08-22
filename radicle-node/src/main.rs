use std::io;
use std::{env, fs, net, path::PathBuf, process};

use anyhow::Context;
use crossbeam_channel as chan;

use radicle::prelude::Signer;
use radicle::profile;
use radicle::version;
use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::Runtime;
use radicle_node::{logger, signals};

pub const NAME: &str = "radicle-node";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HEAD: &str = env!("GIT_HEAD");

pub const HELP_MSG: &str = r#"
Usage

   radicle-node [<option>...]

   If you're running a public seed node, make sure to use `--listen` to bind a listening socket to
   eg. `0.0.0.0:8776`, and add your external addresses in your configuration.

Options

    --config             <path>         Config file to use (default ~/.radicle/config.json)
    --force                             Force start even if an existing control socket is found
    --listen             <address>      Address to listen on
    --version                           Print program version
    --help                              Print help
"#;

#[derive(Debug)]
struct Options {
    config: Option<PathBuf>,
    listen: Vec<net::SocketAddr>,
    force: bool,
}

impl Options {
    fn from_env() -> Result<Self, anyhow::Error> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut listen = Vec::new();
        let mut config = None;
        let mut force = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("force") => {
                    force = true;
                }
                Long("config") => {
                    let value = parser.value()?;
                    let path = PathBuf::from(value);
                    config = Some(path);
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

        Ok(Self {
            force,
            listen,
            config,
        })
    }
}

fn execute() -> anyhow::Result<()> {
    logger::init(log::Level::Debug)?;

    let home = profile::home()?;
    let options = Options::from_env()?;

    log::info!(target: "node", "Starting node..");
    log::info!(target: "node", "Version {} ({})", env!("CARGO_PKG_VERSION"), env!("GIT_HEAD"));
    log::info!(target: "node", "Unlocking node keystore..");

    let passphrase = profile::env::passphrase();
    let keystore = Keystore::new(&home.keys());
    let signer = MemorySigner::load(&keystore, passphrase).context("couldn't load secret key")?;

    log::info!(target: "node", "Node ID is {}", signer.public_key());

    let config = options.config.unwrap_or_else(|| home.config());
    let config = profile::Config::load(&config)?.node;
    let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
    let listen: Vec<std::net::SocketAddr> = if !options.listen.is_empty() {
        options.listen.clone()
    } else {
        config.listen.clone()
    };

    let (notify, signals) = chan::bounded(1);
    signals::install(notify)?;

    if options.force {
        log::debug!(target: "node", "Removing existing control socket..");
        fs::remove_file(home.socket()).ok();
    }
    Runtime::init(home, config, listen, proxy, signals, signer)?.run()?;

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
