use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::exit;

use crossbeam_channel as chan;
use thiserror::Error;

use radicle::node::device::Device;
use radicle::profile;

use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::{Runtime, VERSION};
#[cfg(unix)]
use radicle_signals as signals;

/// The log level to use before reading any other value
/// from configuration.
///
/// Note that this is different from the default value
/// of the command line argument `--log`, as it is valid
/// *even before that argument is parsed*.
/// It ensures that we log the errors parsing the
/// command line arguments, such as `--log`.
const LOG_LEVEL_DEFAULT: &log::Level = &log::Level::Warn;

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

struct Options {
    config: Option<PathBuf>,
    listen: Vec<SocketAddr>,
    log: Option<log::Level>,
    force: bool,
}

fn parse_options() -> Result<Options, lexopt::Error> {
    use lexopt::prelude::*;
    use std::str::FromStr as _;

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
                config = Some(parser.value()?.parse_with(PathBuf::from_str)?);
            }
            Long("listen") => {
                let addr = parser.value()?.parse_with(SocketAddr::from_str)?;
                listen.push(addr);
            }
            Long("log") => {
                log = Some(parser.value()?.parse_with(log::Level::from_str)?);
            }
            Long("help") | Short('h') => {
                println!("{HELP_MSG}");
                exit(0);
            }
            Long("version") => {
                let _ = VERSION.write(&mut io::stdout());
                exit(0);
            }
            _ => {
                return Err(arg.unexpected());
            }
        }
    }

    Ok(Options {
        force,
        listen,
        log,
        config,
    })
}

#[derive(Error, Debug)]
enum ExecutionError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ConfigurationLoading(#[from] profile::config::LoadError),
    #[error(transparent)]
    MemorySigner(#[from] radicle::crypto::ssh::keystore::MemorySignerError),
    #[error(transparent)]
    Runtime(#[from] radicle_node::runtime::Error),
}

fn execute(options: Options) -> Result<(), ExecutionError> {
    let home = profile::home()?;

    // Up to now, the active log level was `LOG_LEVEL_DEFAULT`.
    // The first thing we do after reading command line options is
    // to set the log level, as this influences logging during
    // configuration loading.
    if let Some(level) = options.log {
        log::set_max_level(level.to_level_filter());
    }

    let config = options.config.unwrap_or_else(|| home.config());
    let mut config = profile::Config::load(&config)?;

    if options.log.is_none() {
        log::set_max_level(log::Level::from(config.node.log).to_level_filter());
    } else {
        // It might seem counter-intuitive at first, as there
        // always is a log level in the configuration, but the command
        // line argument has precedence, and if it is present, the
        // log level has been already set above. Thus, we have nothing
        // to do in this case.
    }

    log::info!(target: "node", "Starting node..");
    log::info!(target: "node", "Version {} ({})", env!("RADICLE_VERSION"), env!("GIT_HEAD"));
    log::info!(target: "node", "Unlocking node keystore..");

    let passphrase = profile::env::passphrase();
    let keystore = Keystore::new(&home.keys());
    let signer = Device::from(MemorySigner::load(&keystore, passphrase)?);

    log::info!(target: "node", "Node ID is {}", signer.public_key());

    // Add the preferred seeds as persistent peers so that we reconnect to them automatically.
    config.node.connect.extend(config.preferred_seeds);

    let listen = if !options.listen.is_empty() {
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
        std::fs::remove_file(home.socket()).ok();
    }
    Runtime::init(home, config.node, listen, signals, signer)?.run()?;

    Ok(())
}

fn initialize_logging() {
    let level = *LOG_LEVEL_DEFAULT;

    //  - We are compiling conditionally, so cannot depend
    //    on the concrete type of the logger(s).
    //  - We are dealing with `Option`, so we need `Box: Sized`.
    //  - We want to provide a `Box` to `log::set_boxed_logger`.
    //  - We also want to keep around any errors along the way.
    type Logger = Box<dyn log::Log>;
    type Error = Box<dyn std::error::Error>;

    let journal: Result<Option<Logger>, Error> = {
        #[cfg(all(feature = "systemd", target_os = "linux"))]
        {
            use thiserror::Error;

            #[derive(Error, Debug)]
            #[error("Error connecting to systemd journal: {0}")]
            struct JournalError(io::Error);

            radicle_systemd::journal::logger::<&str, &str, _>("radicle-node".to_string(), [])
                .map_err(|err| Box::new(JournalError(err)) as Error)
        }
        #[cfg(not(all(feature = "systemd", target_os = "linux")))]
        {
            // This is constant, and `rustc` will hopefully use it to
            // optimize away the `match` below.
            Ok(None)
        }
    };

    let (logger, err) = match journal {
        Ok(Some(logger)) => (logger, None),
        otherwise => (
            Box::new(radicle::logger::Logger::new(level)) as Logger,
            otherwise.err(),
        ),
    };

    log::set_boxed_logger(logger).expect("no other logger should have been set already");
    log::set_max_level(level.to_level_filter());

    if let Some(err) = err {
        log::warn!(target: "node", "Error initializing logger (fell back to default): {err}");
    }
}

fn main() {
    // If `RUST_BACKTRACE` does not have a value, then we set it to capture
    // backtraces for better debugging, otherwise we keep the environments
    // value.
    const RUST_BACKTRACE: &str = "RUST_BACKTRACE";
    if std::env::var_os(RUST_BACKTRACE).is_none() {
        std::env::set_var(RUST_BACKTRACE, "1");
    }

    initialize_logging();

    let options = match parse_options() {
        Ok(options) => options,
        Err(err) => {
            // The lexopt errors read nicely with a comma.
            log::error!(target: "node", "Failed to parse options, {err:#}");
            exit(2);
        }
    };

    if let Err(err) = execute(options) {
        log::error!(target: "node", "{err:#}");
        exit(1);
    }
}
