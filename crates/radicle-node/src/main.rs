use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use crossbeam_channel as chan;
use thiserror::Error;

use radicle::node::device::Device;
use radicle::profile;

use radicle_node::crypto::ssh::keystore::{Keystore, MemorySigner};
use radicle_node::fingerprint::{Fingerprint, FingerprintVerification};
use radicle_node::{Runtime, VERSION};
#[cfg(unix)]
use radicle_signals as signals;

const HELP_MSG: &str = r#"
Usage

   radicle-node [<option>...]

   If you're running a public seed node, make sure to use `--listen` to bind a listening socket to
   eg. `0.0.0.0:8776`, and add your external addresses in your configuration.

Options

    --config      <path>                            Config file to use
                  (default: ~/.radicle/config.json)
    --secret      <path>                            Secret key to use
                  (default ~/.radicle/keys/radicle)
    --force                                         Force start even if an existing control socket
                                                      is found
    --listen      <address>                         Address to listen on
    --log-level   <level>                           Set log level
                  (default: info)
    --log-logger  (radicle | structured | systemd)  Set logger implementation
                  (default: radicle)
    --log-format  json                              Set log format for logger implementation
    --version                                       Print program version
    --help                                          Print help
"#;

#[derive(Debug, Clone)]
enum Logger {
    Radicle,
    #[cfg(feature = "structured-logger")]
    Structured,
    #[cfg(all(feature = "systemd", target_os = "linux"))]
    Systemd,
}

impl Default for Logger {
    fn default() -> Self {
        #[cfg(all(feature = "systemd", target_os = "linux"))]
        if radicle_systemd::journal::connected() {
            return Logger::Systemd;
        }

        Logger::Radicle
    }
}

impl FromStr for Logger {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "radicle" => Ok(Logger::Radicle),
            #[cfg(feature = "structured-logger")]
            "structured" => Ok(Logger::Structured),
            #[cfg(all(feature = "systemd", target_os = "linux"))]
            "systemd" => Ok(Logger::Systemd),
            _ => Err("unknown logger"),
        }
    }
}

#[derive(Clone, Copy)]
enum LogFormat {
    #[cfg(feature = "structured-logger")]
    Json,
}

impl FromStr for LogFormat {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "structured-logger")]
            "json" => Ok(LogFormat::Json),
            _ => Err("unknown log format"),
        }
    }
}

struct LogOptions {
    level: Option<log::Level>,
    logger: Logger,
    format: Option<LogFormat>,
}

struct Options {
    config: Option<PathBuf>,
    secret: Option<PathBuf>,
    listen: Vec<SocketAddr>,
    log: LogOptions,
    force: bool,
}

fn parse_options() -> Result<Options, lexopt::Error> {
    use lexopt::prelude::*;
    use std::str::FromStr as _;

    let mut parser = lexopt::Parser::from_env();
    let mut listen = Vec::new();
    let mut config = None;
    let mut secret = None;
    let mut force = false;
    let mut log_level = None;
    let mut log_logger = Logger::default();
    let mut log_format = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("force") => {
                force = true;
            }
            Long("config") => {
                config = Some(parser.value()?.parse_with(PathBuf::from_str)?);
            }
            Long("secret") => {
                secret = Some(parser.value()?.parse()?);
            }
            Long("listen") => {
                let addr = parser.value()?.parse_with(SocketAddr::from_str)?;
                listen.push(addr);
            }
            Long("log") | Long("log-level") => {
                if matches!(arg, Long("log")) {
                    eprintln!("Warning: The option `--log` is deprecated and will be removed. Please use `--log-level` instead.");
                }
                log_level = Some(parser.value()?.parse_with(log::Level::from_str)?);
            }
            Long("log-logger") => {
                let parsed = parser.value()?.parse_with(Logger::from_str)?;
                if matches!(parsed, Logger::Radicle) {
                    return Err(lexopt::Error::Custom(
                        "explicitly choosing this logger is forbidden, because it is deprecated"
                            .into(),
                    ));
                }
                log_logger = parsed;
            }
            Long("log-format") => {
                log_format = Some(parser.value()?.parse_with(LogFormat::from_str)?);
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
        secret,
        listen,
        config,
        log: LogOptions {
            level: log_level,
            logger: log_logger,
            format: log_format,
        },
    })
}

#[derive(Error, Debug)]
enum ExecutionError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ConfigurationLoading(#[from] profile::config::LoadError),
    #[error(transparent)]
    Runtime(#[from] radicle_node::runtime::Error),
    #[error(transparent)]
    Fingerprint(#[from] radicle_node::fingerprint::Error),
    #[error("failed to load secret key '{path}': not found")]
    SecretNotFound { path: PathBuf },
    #[error("failed to load secret '{path}': {source}")]
    SecretLoading {
        path: PathBuf,
        source: radicle::crypto::ssh::keystore::Error,
    },
    #[error("failed to load secret key '{secret}': fingerprint of corresponding public key is different from '{fingerprint}'")]
    FingerprintMismatch {
        secret: PathBuf,
        fingerprint: Fingerprint,
    },
}

fn execute(options: Options) -> Result<(), ExecutionError> {
    let home = profile::home()?;

    // Up to now, the active log level was `LOG_LEVEL_DEFAULT`.
    // The first thing we do after reading command line options is
    // to set the log level, as this influences logging during
    // configuration loading.
    if let Some(level) = options.log.level {
        log::set_max_level(level.to_level_filter());
    }

    let config = options.config.unwrap_or_else(|| home.config());
    let mut config = profile::Config::load(&config)?;

    if options.log.level.is_none() {
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

    let passphrase = None;

    #[cfg(all(feature = "systemd", target_os = "linux"))]
    let passphrase = passphrase.or_else(|| {
        const ID: &str = "xyz.radicle.node.passphrase";
        match radicle_systemd::credential::path(ID) {
            Err(err) => {
                log::warn!(target: "node", "Failed to obtain path of the passphrase file via systemd credential with '{ID}': {err}");
                None
            },
            Ok(Some(ref path)) => match std::fs::read_to_string(path) {
                Ok(passphrase) => Some(passphrase.into()),
                Err(err) => {
                    log::warn!(target: "node", "Failed to read passphrase from '{}': {err}", path.display());
                    None
                }
            }
            Ok(None) => None,
        }
    });

    let passphrase = passphrase.or_else(profile::env::passphrase);

    let secret_path = options.secret;

    #[cfg(all(feature = "systemd", target_os = "linux"))]
    let secret_path = secret_path.or_else(|| {
        const ID: &str = "xyz.radicle.node.secret";
        match radicle_systemd::credential::path(ID) {
            Err(err) => {
                log::warn!(target: "node", "Failed to obtain path of the secret key via systemd credential with ID '{ID}': {err}");
                None
            },
            Ok(path) => path
        }
    });

    let secret_path = secret_path
        .or_else(|| config.node.secret.clone())
        .unwrap_or_else(|| home.keys().join("radicle"));

    let keystore = Keystore::from_secret_path(&secret_path);

    let secret_key = keystore
        .secret_key(passphrase.clone())
        .map_err(|err| ExecutionError::SecretLoading {
            path: secret_path.clone(),
            source: err,
        })?
        .ok_or_else(|| ExecutionError::SecretNotFound {
            path: secret_path.clone(),
        })?;

    if let Some(fp) = Fingerprint::read(&home)? {
        log::debug!(target: "node", "Verifying fingerprint..");
        if fp.verify(&secret_key) != FingerprintVerification::Match {
            return Err(ExecutionError::FingerprintMismatch {
                secret: keystore.secret_key_path().to_path_buf(),
                fingerprint: fp,
            });
        }
    } else {
        log::info!(target: "node", "Initializing fingerprint..");
        Fingerprint::init(&home, &secret_key)?;
    }

    let signer = Device::from(MemorySigner::from_secret(secret_key));
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

fn initialize_logging(options: &LogOptions) -> Result<(), Box<dyn std::error::Error>> {
    let level = options.level.unwrap_or(log::Level::Info);

    let logger: Box<dyn log::Log> = {
        match options.logger {
            #[cfg(feature = "structured-logger")]
            Logger::Structured => {
                use structured_logger::{json, Builder};

                let writer = match options.format.unwrap_or(LogFormat::Json) {
                    LogFormat::Json => json::new_writer(io::stdout()),
                };

                Box::new(Builder::new().with_default_writer(writer).build())
            }
            #[cfg(all(feature = "systemd", target_os = "linux"))]
            Logger::Systemd => {
                use radicle_systemd::journal::*;
                use thiserror::Error;

                #[derive(Error, Debug)]
                enum JournalError {
                    #[error("journald not connected")]
                    NotConnected,
                    #[error("journald i/o: {0}")]
                    Io(#[from] io::Error),
                }

                if !connected() {
                    return Err(Box::new(JournalError::NotConnected));
                }

                const SYSLOG_IDENTIFIER: &str = "radicle-node";
                logger::<&str, &str, _>(SYSLOG_IDENTIFIER.to_string(), []).map_err(Box::new)?
            }
            Logger::Radicle => Box::new(radicle::logger::Logger::new(level)),
        }
    };

    log::set_boxed_logger(logger).expect("no other logger should have been set already");
    log::set_max_level(level.to_level_filter());

    Ok(())
}

fn panic_hook(info: &std::panic::PanicHookInfo) {
    #[cfg(feature = "backtrace")]
    let backtrace = format!("{:?}", backtrace::Backtrace::new());

    #[cfg(not(feature = "backtrace"))]
    let backtrace = " (no backtrace available)";

    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("<unnamed>");

    let msg = info
        .payload()
        .downcast_ref::<&'static str>()
        .copied()
        .or(info.payload().downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("Box<Any>");

    match info.location() {
        Some(location) => {
            log::error!(
                target: "panic", "thread '{thread}' panicked at '{msg}': {}:{}{backtrace}",
                location.file(),
                location.line(),
            );
        }
        None => log::error!(
            target: "panic", "thread '{thread}' panicked at '{msg}'{backtrace}",
        ),
    }

    log::logger().flush();
}

fn main() {
    let options = parse_options().unwrap_or_else(|err| {
        // The lexopt errors read nicely with a comma.
        eprintln!("Failed to parse options, {err:#}");
        exit(2);
    });

    initialize_logging(&options.log).unwrap_or_else(|err| {
        eprintln!("Failed to initialize logging: {err:#}");
        exit(3);
    });

    std::panic::set_hook(Box::new(panic_hook));

    if let Err(err) = execute(options) {
        log::error!(target: "node", "{err:#}");
        exit(1);
    }
}
