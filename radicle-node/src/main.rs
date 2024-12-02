use std::io;
use std::{env, fs, net, path::PathBuf, process};

use anyhow::Context;
use crossbeam_channel as chan;

use radicle::logger;
use radicle::node::device::Device;
use radicle::profile;
use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::{Runtime, VERSION};
use radicle_signals as signals;

pub const HELP_MSG: &str = r#"
Usage

   radicle-node [<option>...]

   If you're running a public seed node, make sure to use `--listen` to bind a listening socket to
   eg. `0.0.0.0:8776`, and add your external addresses in your configuration.

Options

    --config             <path>         Config file to use (default ~/.radicle/config.json)
    --force                             Force start even if an existing control socket is found
    --listen             <address>      Address to listen on
    --log                <level>        Set log level (default: info)
    --version                           Print program version
    --help                              Print help
"#;

#[derive(Debug)]
struct Options {
    config: Option<PathBuf>,
    listen: Vec<net::SocketAddr>,
    log: Option<log::Level>,
    force: bool,
}

impl Options {
    fn from_env() -> Result<Self, anyhow::Error> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut listen = Vec::new();
        let mut config = None;
        let mut force = false;
        let mut log = None;

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
                Long("log") => {
                    log = Some(parser.value()?.parse()?);
                }
                Long("help") | Short('h') => {
                    println!("{HELP_MSG}");
                    process::exit(0);
                }
                Long("version") => {
                    VERSION.write(&mut io::stdout())?;
                    process::exit(0);
                }
                _ => anyhow::bail!(arg.unexpected()),
            }
        }

        Ok(Self {
            force,
            listen,
            log,
            config,
        })
    }
}

fn execute() -> anyhow::Result<()> {
    let home = profile::home()?;
    let options = Options::from_env()?;
    let config = options.config.unwrap_or_else(|| home.config());
    let mut config = profile::Config::load(&config)?;

    logger::init(options.log.unwrap_or(config.node.log))?;

    log::info!(target: "node", "Starting node..");
    log::info!(target: "node", "Version {} ({})", env!("RADICLE_VERSION"), env!("GIT_HEAD"));
    log::info!(target: "node", "Unlocking node keystore..");

    let passphrase = profile::env::passphrase();
    let keystore = Keystore::new(&home.keys());
    let signer = Device::from(
        MemorySigner::load(&keystore, passphrase).context("couldn't load secret key")?,
    );

    log::info!(target: "node", "Node ID is {}", signer.public_key());

    // Add the preferred seeds as persistent peers so that we reconnect to them automatically.
    config.node.connect.extend(config.preferred_seeds);

    let listen: Vec<std::net::SocketAddr> = if !options.listen.is_empty() {
        options.listen.clone()
    } else {
        config.node.listen.clone()
    };

    if let Err(e) = radicle::io::set_file_limit(config.node.limits.max_open_files) {
        log::warn!(target: "node", "Unable to set process open file limit: {e}");
    }

    let (notify, signals) = chan::bounded(1);
    signals::install(notify)?;

    if options.force {
        log::debug!(target: "node", "Removing existing control socket..");
        fs::remove_file(home.socket()).ok();
    }
    Runtime::init(home, config.node, listen, signals, signer)?.run()?;

    Ok(())
}

fn main() {
    if let Err(err) = execute() {
        if log::log_enabled!(target: "node", log::Level::Error) {
            log::error!(target: "node", "Fatal: {err:#}");
        } else {
            eprintln!("Error: {err:#}");
        }
        process::exit(1);
    }
}
