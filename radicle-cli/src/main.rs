use std::ffi::OsString;
use std::io::{self, Write};
use std::{io::ErrorKind, iter, process};

use anyhow::anyhow;
use clap::builder::styling::Style;
use clap::builder::Styles;
use clap::{CommandFactory, Parser, Subcommand};

use radicle::version;
use radicle_cli::cli;
use radicle_cli::commands::rad_issue;
use radicle_cli::commands::*;
use radicle_cli::terminal as term;

pub const NAME: &str = "rad";
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RADICLE_VERSION: &str = env!("RADICLE_VERSION");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const LONG_DESCRIPTION: &str = "Radicle is a distributed GIT forge.";
pub const GIT_HEAD: &str = env!("GIT_HEAD");
pub const TIMESTAMP: &str = env!("GIT_COMMIT_TIME");
pub const VERSION: Version = Version {
    name: NAME,
    version: RADICLE_VERSION,
    commit: GIT_HEAD,
    timestamp: TIMESTAMP,
};
pub const LONG_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HEAD"), ")");
pub const HELP_TEMPLATE: &str = r#"
{before-help}{bin} {version}
{about-with-newline}
Usage: {usage}

{all-args}
{after-help}
"#;

/// Radicle command line interface
///
/// Radicle is a distributed GIT forge.
#[derive(Parser, Debug)]
#[command(name = NAME)]
#[command(version = VERSION)]
#[command(long_version = LONG_VERSION)]
#[command(help_template = HELP_TEMPLATE)]
#[command(propagate_version = true)]
#[command(propagate_help_template = true)]
// #[command(styles = Styles::styled().usage(AnsiColor::Blue.on_default()))]
#[command(styles = Styles::plain().literal(Style::new().bold()))]
struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Manage issues
    ///
    /// With issues you can organize your project and use it to discuss bugs and improvements.
    Issue(rad_issue::IssueArgs),
}

#[derive(Debug)]
enum Command {
    Other(Vec<OsString>),
    Help,
    Version { json: bool },
}

fn main() {
    if let Some(lvl) = radicle::logger::env_level() {
        radicle::logger::init(lvl).ok();
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
            _ => return Err(anyhow::anyhow!(arg.unexpected())),
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

    rad_help::run(Default::default(), term::DefaultContext)
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

fn run_other(exe: &str, args: &[OsString]) -> Result<(), Option<anyhow::Error>> {
    match exe {
        "auth" => {
            term::run_command_args::<rad_auth::Options, _>(
                rad_auth::HELP,
                rad_auth::run,
                args.to_vec(),
            );
        }
        "block" => {
            term::run_command_args::<rad_block::Options, _>(
                rad_block::HELP,
                rad_block::run,
                args.to_vec(),
            );
        }
        "checkout" => {
            term::run_command_args::<rad_checkout::Options, _>(
                rad_checkout::HELP,
                rad_checkout::run,
                args.to_vec(),
            );
        }
        "clone" => {
            term::run_command_args::<rad_clone::Options, _>(
                rad_clone::HELP,
                rad_clone::run,
                args.to_vec(),
            );
        }
        "cob" => {
            term::run_command_args::<rad_cob::Options, _>(
                rad_cob::HELP,
                rad_cob::run,
                args.to_vec(),
            );
        }
        "config" => {
            term::run_command_args::<rad_config::Options, _>(
                rad_config::HELP,
                rad_config::run,
                args.to_vec(),
            );
        }
        "diff" => {
            term::run_command_args::<rad_diff::Options, _>(
                rad_diff::HELP,
                rad_diff::run,
                args.to_vec(),
            );
        }
        "debug" => {
            term::run_command_args::<rad_debug::Options, _>(
                rad_debug::HELP,
                rad_debug::run,
                args.to_vec(),
            );
        }
        "follow" => {
            term::run_command_args::<rad_follow::Options, _>(
                rad_follow::HELP,
                rad_follow::run,
                args.to_vec(),
            );
        }
        "fork" => {
            term::run_command_args::<rad_fork::Options, _>(
                rad_fork::HELP,
                rad_fork::run,
                args.to_vec(),
            );
        }
        "help" => {
            term::run_command_args::<rad_help::Options, _>(
                rad_help::HELP,
                rad_help::run,
                args.to_vec(),
            );
        }
        "id" => {
            term::run_command_args::<rad_id::Options, _>(rad_id::HELP, rad_id::run, args.to_vec());
        }
        "inbox" => term::run_command_args::<rad_inbox::Options, _>(
            rad_inbox::HELP,
            rad_inbox::run,
            args.to_vec(),
        ),
        "init" => {
            term::run_command_args::<rad_init::Options, _>(
                rad_init::HELP,
                rad_init::run,
                args.to_vec(),
            );
        }
        "inspect" => {
            term::run_command_args::<rad_inspect::Options, _>(
                rad_inspect::HELP,
                rad_inspect::run,
                args.to_vec(),
            );
        }
        "issue" => {
            let args_ = CliArgs::parse();
            if let Some(command) = args_.command {
                match command {
                    Commands::Issue(args_) => rad_issue::run(
                        args_,
                        radicle::Profile::load()
                            .map_err(|e| anyhow!(e))?,
                    )?,
                }
                // If clap parsed a command short circuit.
                // return Ok(());
            }
        }
        "complete" => {
            cli::completer(CliArgs::command(), args.to_vec());
        }
        "ls" => {
            term::run_command_args::<rad_ls::Options, _>(rad_ls::HELP, rad_ls::run, args.to_vec());
        }
        "node" => {
            term::run_command_args::<rad_node::Options, _>(
                rad_node::HELP,
                rad_node::run,
                args.to_vec(),
            );
        }
        "patch" => {
            term::run_command_args::<rad_patch::Options, _>(
                rad_patch::HELP,
                rad_patch::run,
                args.to_vec(),
            );
        }
        "path" => {
            term::run_command_args::<rad_path::Options, _>(
                rad_path::HELP,
                rad_path::run,
                args.to_vec(),
            );
        }
        "publish" => {
            term::run_command_args::<rad_publish::Options, _>(
                rad_publish::HELP,
                rad_publish::run,
                args.to_vec(),
            );
        }
        "clean" => {
            term::run_command_args::<rad_clean::Options, _>(
                rad_clean::HELP,
                rad_clean::run,
                args.to_vec(),
            );
        }
        "self" => {
            term::run_command_args::<rad_self::Options, _>(
                rad_self::HELP,
                rad_self::run,
                args.to_vec(),
            );
        }
        "sync" => {
            term::run_command_args::<rad_sync::Options, _>(
                rad_sync::HELP,
                rad_sync::run,
                args.to_vec(),
            );
        }
        "seed" => {
            term::run_command_args::<rad_seed::Options, _>(
                rad_seed::HELP,
                rad_seed::run,
                args.to_vec(),
            );
        }
        "unfollow" => {
            term::run_command_args::<rad_unfollow::Options, _>(
                rad_unfollow::HELP,
                rad_unfollow::run,
                args.to_vec(),
            );
        }
        "unseed" => {
            term::run_command_args::<rad_unseed::Options, _>(
                rad_unseed::HELP,
                rad_unseed::run,
                args.to_vec(),
            );
        }
        "remote" => term::run_command_args::<rad_remote::Options, _>(
            rad_remote::HELP,
            rad_remote::run,
            args.to_vec(),
        ),
        "stats" => term::run_command_args::<rad_stats::Options, _>(
            rad_stats::HELP,
            rad_stats::run,
            args.to_vec(),
        ),
        "watch" => term::run_command_args::<rad_watch::Options, _>(
            rad_watch::HELP,
            rad_watch::run,
            args.to_vec(),
        ),
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
