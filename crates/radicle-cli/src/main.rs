use std::ffi::OsString;
use std::io::{self, Write};
use std::{io::ErrorKind, iter, process};

use anyhow::anyhow;
use clap::builder::styling::Style;
use clap::builder::Styles;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use radicle::version::Version;
use radicle_cli::commands::rad_issue;
use radicle_cli::commands::patch;
use radicle_cli::commands::*;
use radicle_cli::terminal as term;

pub const NAME: &str = "rad";
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RADICLE_VERSION: &str = env!("RADICLE_VERSION");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const LONG_DESCRIPTION: &str = "Radicle is a sovereign code forge built on Git.";
pub const GIT_HEAD: &str = env!("GIT_HEAD");
pub const TIMESTAMP: &str = env!("SOURCE_DATE_EPOCH");
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
#[derive(Parser, Debug)]
#[command(name = NAME)]
#[command(version = PKG_VERSION)]
#[command(long_version = LONG_VERSION)]
#[command(help_template = HELP_TEMPLATE)]
#[command(propagate_version = true)]
#[command(styles = Styles::plain().literal(Style::new().bold()))]
struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(long = "generate")]
    #[clap(global = true)]
    pub(crate) generator: Option<clap_complete::Shell>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Commands to create, view, and edit Radicle issues
    ///
    /// With issues you can organize your project and use it to discuss bugs and improvements.
    Issue(rad_issue::Args),

    /// Commands to create, view, and edit Radicle patches
    Patch(rad_patch::Args),
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

fn run_other(exe: &str, args: &[OsString]) -> Result<(), Option<anyhow::Error>> {
    match exe {
        "auth" => {
            term::run_command_args::<auth::Options, _>(auth::HELP, auth::run, args.to_vec());
        }
        "block" => {
            term::run_command_args::<block::Options, _>(block::HELP, block::run, args.to_vec());
        }
        "checkout" => {
            term::run_command_args::<checkout::Options, _>(
                checkout::HELP,
                checkout::run,
                args.to_vec(),
            );
        }
        "clone" => {
            term::run_command_args::<clone::Options, _>(clone::HELP, clone::run, args.to_vec());
        }
        "cob" => {
            term::run_command_args::<cob::Options, _>(cob::HELP, cob::run, args.to_vec());
        }
        "config" => {
            term::run_command_args::<config::Options, _>(config::HELP, config::run, args.to_vec());
        }
        "diff" => {
            term::run_command_args::<diff::Options, _>(diff::HELP, diff::run, args.to_vec());
        }
        "debug" => {
            term::run_command_args::<debug::Options, _>(debug::HELP, debug::run, args.to_vec());
        }
        "follow" => {
            term::run_command_args::<follow::Options, _>(follow::HELP, follow::run, args.to_vec());
        }
        "fork" => {
            term::run_command_args::<fork::Options, _>(fork::HELP, fork::run, args.to_vec());
        }
        "help" => {
            term::run_command_args::<help::Options, _>(help::HELP, help::run, args.to_vec());
        }
        "id" => {
            term::run_command_args::<id::Options, _>(id::HELP, id::run, args.to_vec());
        }
        "inbox" => {
            term::run_command_args::<inbox::Options, _>(inbox::HELP, inbox::run, args.to_vec())
        }
        "init" => {
            term::run_command_args::<init::Options, _>(init::HELP, init::run, args.to_vec());
        }
        "inspect" => {
            term::run_command_args::<inspect::Options, _>(
                inspect::HELP,
                inspect::run,
                args.to_vec(),
            );
        }
        "issue" => {
            // Use clap instead to parse all CLI args and ignore `args` passed
            // to `run_other`.
            let args_ = CliArgs::parse();
            if let Some(command) = args_.command {
                match command {
                    Commands::Issue(args_) => {
                        if let Err(err) =
                            issue::run(args_, radicle::Profile::load().map_err(|e| anyhow!(e))?)
                        {
                            radicle_cli::terminal::fail("", &err);
                            process::exit(1);
                        }
                    }
                }
            }
        }
        // Used for dynamic shell completion (not user facing)
        "complete" => {
            println!("parsing args");
            let args_ = CliArgs::parse_from(args);
            println!("have args");
            let generator = args_.generator.unwrap_or(Shell::Fish);
            let mut cmd = CliArgs::command();
            let bin_name = cmd.get_name().to_string();
            clap_complete::generate(generator, &mut cmd, bin_name, &mut io::stdout());
        }
        "ls" => {
            term::run_command_args::<ls::Options, _>(ls::HELP, ls::run, args.to_vec());
        }
        "node" => {
            term::run_command_args::<node::Options, _>(node::HELP, node::run, args.to_vec());
        }
        "patch" => {
            term::run_command_args::<patch::Options, _>(patch::HELP, patch::run, args.to_vec());
        }
        "path" => {
            term::run_command_args::<path::Options, _>(path::HELP, path::run, args.to_vec());
        }
        "publish" => {
            term::run_command_args::<publish::Options, _>(
                publish::HELP,
                publish::run,
                args.to_vec(),
            );
        }
        "clean" => {
            term::run_command_args::<clean::Options, _>(clean::HELP, clean::run, args.to_vec());
        }
        "self" => {
            term::run_command_args::<rad_self::Options, _>(
                rad_self::HELP,
                rad_self::run,
                args.to_vec(),
            );
        }
        "sync" => {
            term::run_command_args::<sync::Options, _>(sync::HELP, sync::run, args.to_vec());
        }
        "seed" => {
            term::run_command_args::<seed::Options, _>(seed::HELP, seed::run, args.to_vec());
        }
        "unblock" => {
            term::run_command_args::<unblock::Options, _>(
                unblock::HELP,
                unblock::run,
                args.to_vec(),
            );
        }
        "unfollow" => {
            term::run_command_args::<unfollow::Options, _>(
                unfollow::HELP,
                unfollow::run,
                args.to_vec(),
            );
        }
        "unseed" => {
            term::run_command_args::<unseed::Options, _>(unseed::HELP, unseed::run, args.to_vec());
        }
        "remote" => {
            term::run_command_args::<remote::Options, _>(remote::HELP, remote::run, args.to_vec())
        }
        "stats" => {
            term::run_command_args::<stats::Options, _>(stats::HELP, stats::run, args.to_vec())
        }
        "watch" => {
            term::run_command_args::<watch::Options, _>(watch::HELP, watch::run, args.to_vec())
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
