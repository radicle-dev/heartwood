use std::io;
use std::{env, fs, net, path::PathBuf, process};

use anyhow::Context;
use crossbeam_channel as chan;

use radicle::node::device::Device;
use radicle::profile;
use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::{Runtime, VERSION};
#[cfg(unix)]
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

    let level = options.log.unwrap_or_else(|| config.node.log.into());

    let logger = {
        let journal = {
            #[cfg(all(feature = "systemd", target_os = "linux"))]
            {
                radicle_systemd::journal::logger::<&str, &str, _>("radicle-node".to_string(), [])?
            }
            #[cfg(not(all(feature = "systemd", target_os = "linux")))]
            {
                None
            }
        };

        if let Some(logger) = journal {
            logger
        } else {
            Box::new(radicle::logger::Logger::new(level))
        }
    };

    log::set_boxed_logger(logger).expect("no other logger should have been set already");
    log::set_max_level(level.to_level_filter());

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

    if let Err(e) = radicle::io::set_file_limit::<usize>(config.node.limits.max_open_files.into()) {
        log::warn!(target: "node", "Unable to set process open file limit: {e}");
    }

    #[cfg(unix)]
    let signals = {
        let (notify, signals) = chan::bounded(1);
        signals::install(notify)?;
        signals
    };

    #[cfg(windows)]
    let signals = {
        let (_, signals) = chan::bounded(1);
        log::warn!(target: "node", "Signal handlers not installed.");
        signals
    };

    if options.force {
        log::debug!(target: "node", "Removing existing control socket..");
        fs::remove_file(home.socket()).ok();
    }
    Runtime::init(home, config.node, listen, signals, signer)?.run()?;

    Ok(())
}

fn main() {
    // If `RUST_BACKTRACE` does not have a value, then we set it to capture
    // backtraces for better debugging, otherwise we keep the environments
    // value.
    const RUST_BACKTRACE: &str = "RUST_BACKTRACE";
    if std::env::var_os(RUST_BACKTRACE).is_none() {
        std::env::set_var(RUST_BACKTRACE, "1");
    }

    if let Err(err) = execute() {
        log::error!(target: "node", "{err:#}");
        process::exit(1);
    }
}
