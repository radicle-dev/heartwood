use std::ffi::OsString;
use std::io::{self, Write};
use std::{io::ErrorKind, iter, process};

use anyhow::anyhow;
use clap::builder::styling::AnsiColor;
use clap::builder::Styles;
use clap::{Parser, Subcommand};

use radicle::version::Version;
use radicle_cli::commands::*;
use radicle_cli::terminal as term;

pub const NAME: &str = "rad";
pub const GIT_HEAD: &str = env!("GIT_HEAD");
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RADICLE_VERSION: &str = env!("RADICLE_VERSION");
pub const RADICLE_VERSION_LONG: &str =
    concat!(env!("RADICLE_VERSION"), " (", env!("GIT_HEAD"), ")");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const LONG_DESCRIPTION: &str = "Radicle is a sovereign code forge built on Git.";
pub const TIMESTAMP: &str = env!("SOURCE_DATE_EPOCH");
pub const VERSION: Version = Version {
    name: NAME,
    version: RADICLE_VERSION,
    commit: GIT_HEAD,
    timestamp: TIMESTAMP,
};
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Magenta.on_default().bold())
    .usage(AnsiColor::Magenta.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());

/// Radicle command line interface
#[derive(Parser, Debug)]
#[command(name = NAME)]
#[command(version = RADICLE_VERSION)]
#[command(long_version = RADICLE_VERSION_LONG)]
#[command(propagate_version = true)]
#[command(styles = STYLES)]
struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Auth(auth::Args),
    Block(block::Args),
    Checkout(checkout::Args),
    Clean(clean::Args),
    Clone(clone::Args),
    #[command(hide = true)]
    Cob(cob::Args),
    Debug(debug::Args),

    /// This command is deprecated and delegates to `git diff`.
    /// Even before it was deprecated, it was not printed by
    /// `rad -h`, so it is also hidden.
    ///
    /// Since it is hidden, it makes no sense to add `about`
    /// for the command listing, and since it is external,
    /// `--help` will delegate to `git diff --help` it makes
    /// no sense to add `long_about` for `rad diff --help`.
    #[command(external_subcommand, hide = true)]
    Diff(Vec<OsString>),

    Follow(follow::Args),
    Fork(fork::Args),
    Id(id::Args),
    Init(init::Args),
    Issue(issue::Args),
    Ls(ls::Args),
    Path(path::Args),
    Publish(publish::Args),
    Seed(seed::Args),
    #[command(name = "self")]
    RadSelf(rad_self::Args),
    Stats(stats::Args),
    Unblock(unblock::Args),
    Unfollow(unfollow::Args),
    Unseed(unseed::Args),
    Watch(watch::Args),
}

#[derive(Debug)]
enum Command {
    Other(Vec<OsString>),
    Help,
    Version { json: bool },
}

fn main() {
    human_panic::setup_panic!(human_panic::Metadata::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )
    .homepage(env!("CARGO_PKG_HOMEPAGE"))
    .support("Open a support request at https://radicle.zulipchat.com/ or file an issue via Radicle itself, or e-mail to team@radicle.xyz"));

    if let Some(lvl) = radicle::logger::env_level() {
        let logger = Box::new(radicle::logger::Logger::new(lvl));
        log::set_boxed_logger(logger).expect("no other logger should have been set already");
        log::set_max_level(lvl.to_level_filter());
    }
    if let Err(e) = radicle::io::set_file_limit(4096) {
        log::warn!(target: "cli", "Unable to set open file limit: {e}");
    }
    match parse_args().map_err(Some).and_then(run) {
        Ok(_) => process::exit(0),
        Err(err) => {
            if let Some(err) = err {
                term::error(format!("rad: {err}"));
            }
            process::exit(1);
        }
    }
}

fn parse_args() -> anyhow::Result<Command> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut command = None;
    let mut json = false;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("json") => {
                json = true;
            }
            Long("help") | Short('h') => {
                command = Some(Command::Help);
            }
            Long("version") => {
                command = Some(Command::Version { json: false });
            }
            Value(val) if command.is_none() => {
                if val == *"." {
                    command = Some(Command::Other(vec![OsString::from("inspect")]));
                } else if val == "version" {
                    command = Some(Command::Version { json: false });
                } else {
                    let args = iter::once(val)
                        .chain(iter::from_fn(|| parser.value().ok()))
                        .collect();

                    command = Some(Command::Other(args))
                }
            }
            _ => anyhow::bail!(arg.unexpected()),
        }
    }
    if let Some(Command::Version { json: j }) = &mut command {
        *j = json;
    }
    Ok(command.unwrap_or_else(|| Command::Other(vec![])))
}

fn print_help() -> anyhow::Result<()> {
    VERSION.write(&mut io::stdout())?;
    println!("{DESCRIPTION}");
    println!();

    help::run(Default::default(), term::DefaultContext)
}

fn run(command: Command) -> Result<(), Option<anyhow::Error>> {
    match command {
        Command::Version { json } => {
            let mut stdout = io::stdout();
            if json {
                VERSION
                    .write_json(&mut stdout)
                    .map_err(|e| Some(e.into()))?;
                writeln!(&mut stdout).ok();
            } else {
                VERSION.write(&mut stdout).map_err(|e| Some(e.into()))?;
            }
        }
        Command::Help => {
            print_help()?;
        }
        Command::Other(args) => {
            let exe = args.first();

            if let Some(Some(exe)) = exe.map(|s| s.to_str()) {
                run_other(exe, &args[1..])?;
            } else {
                print_help()?;
            }
        }
    }

    Ok(())
}

/// Runs a `rad` command. `exe` expects the commands' name, e.g. `issue`,
/// `args` expects all other arguments.
///
/// For commands that are already migrated to `clap`, we need to parse the
/// arguments again. This needs to be done for each migrated command
/// individually, otherwise `clap` would fail to parse on an non-migrated and
/// therefore unknown command.
pub(crate) fn run_other(exe: &str, args: &[OsString]) -> Result<(), Option<anyhow::Error>> {
    match exe {
        "auth" => {
            if let Some(Commands::Auth(args)) = CliArgs::parse().command {
                term::run_command_fn(auth::run, args);
            }
        }
        "block" => {
            if let Some(Commands::Block(args)) = CliArgs::parse().command {
                term::run_command_fn(block::run, args);
            }
        }
        "checkout" => {
            if let Some(Commands::Checkout(args)) = CliArgs::parse().command {
                term::run_command_fn(checkout::run, args);
            }
        }
        "clone" => {
            if let Some(Commands::Clone(args)) = CliArgs::parse().command {
                term::run_command_fn(clone::run, args);
            }
        }
        "cob" => {
            if let Some(Commands::Cob(args)) = CliArgs::parse().command {
                term::run_command_fn(cob::run, args);
            }
        }
        "config" => {
            term::run_command_args::<config::Options, _>(config::HELP, config::run, args.to_vec());
        }
        "diff" => {
            if let Some(Commands::Diff(mut args)) = CliArgs::parse().command {
                debug_assert_eq!(args[0], "diff");
                args.remove(0);
                return diff::run(args).map_err(Some);
            }
        }
        "debug" => {
            if let Some(Commands::Debug(args)) = CliArgs::parse().command {
                term::run_command_fn(debug::run, args);
            }
        }
        "follow" => {
            if let Some(Commands::Follow(args)) = CliArgs::parse().command {
                term::run_command_fn(follow::run, args);
            }
        }
        "fork" => {
            if let Some(Commands::Fork(args)) = CliArgs::parse().command {
                term::run_command_fn(fork::run, args);
            }
        }
        "help" => {
            term::run_command_args::<help::Options, _>(help::HELP, help::run, args.to_vec());
        }
        "id" => {
            if let Some(Commands::Id(args)) = CliArgs::parse().command {
                term::run_command_fn(id::run, args);
            }
        }
        "inbox" => {
            term::run_command_args::<inbox::Options, _>(inbox::HELP, inbox::run, args.to_vec())
        }
        "init" => {
            if let Some(Commands::Init(args)) = CliArgs::parse().command {
                term::run_command_fn(init::run, args);
            }
        }
        "inspect" => {
            term::run_command_args::<inspect::Options, _>(
                inspect::HELP,
                inspect::run,
                args.to_vec(),
            );
        }
        "issue" => {
            if let Some(Commands::Issue(args)) = CliArgs::parse().command {
                term::run_command_fn(issue::run, args);
            }
        }
        "ls" => {
            if let Some(Commands::Ls(args)) = CliArgs::parse().command {
                term::run_command_fn(ls::run, args);
            }
        }
        "node" => {
            term::run_command_args::<node::Options, _>(node::HELP, node::run, args.to_vec());
        }
        "patch" => {
            term::run_command_args::<patch::Options, _>(patch::HELP, patch::run, args.to_vec());
        }
        "path" => {
            if let Some(Commands::Path(args)) = CliArgs::parse().command {
                term::run_command_fn(path::run, args);
            }
        }
        "publish" => {
            if let Some(Commands::Publish(args)) = CliArgs::parse().command {
                term::run_command_fn(publish::run, args);
            }
        }
        "clean" => {
            if let Some(Commands::Clean(args)) = CliArgs::parse().command {
                term::run_command_fn(clean::run, args);
            }
        }
        "self" => {
            if let Some(Commands::RadSelf(args)) = CliArgs::parse().command {
                term::run_command_fn(rad_self::run, args)
            }
        }
        "sync" => {
            term::run_command_args::<sync::Options, _>(sync::HELP, sync::run, args.to_vec());
        }
        "seed" => {
            if let Some(Commands::Seed(args)) = CliArgs::parse().command {
                term::run_command_fn(seed::run, args);
            }
        }
        "unblock" => {
            if let Some(Commands::Unblock(args)) = CliArgs::parse().command {
                term::run_command_fn(unblock::run, args);
            }
        }
        "unfollow" => {
            if let Some(Commands::Unfollow(args)) = CliArgs::parse().command {
                term::run_command_fn(unfollow::run, args);
            }
        }
        "unseed" => {
            if let Some(Commands::Unseed(args)) = CliArgs::parse().command {
                term::run_command_fn(unseed::run, args);
            }
        }
        "remote" => {
            term::run_command_args::<remote::Options, _>(remote::HELP, remote::run, args.to_vec())
        }
        "stats" => {
            if let Some(Commands::Stats(args)) = CliArgs::parse().command {
                term::run_command_fn(stats::run, args);
            }
        }
        "watch" => {
            if let Some(Commands::Watch(args)) = CliArgs::parse().command {
                term::run_command_fn(watch::run, args);
            }
        }
        other => {
            let exe = format!("{NAME}-{exe}");
            let status = process::Command::new(exe).args(args).status();

            match status {
                Ok(status) => {
                    if !status.success() {
                        return Err(None);
                    }
                }
                Err(err) => {
                    if let ErrorKind::NotFound = err.kind() {
                        return Err(Some(anyhow!(
                            "`{other}` is not a command. See `rad --help` for a list of commands.",
                        )));
                    } else {
                        return Err(Some(err.into()));
                    }
                }
            }
        }
    }
    Ok(())
}
