use std::ffi::OsString;
use std::{io::ErrorKind, iter, process};

use anyhow::anyhow;

use radicle_cli::commands::*;
use radicle_cli::terminal as term;

use auth as rad_auth;
use checkout as rad_checkout;
use help as rad_help;
use init as rad_init;

pub const NAME: &str = "rad";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const GIT_HEAD: &str = env!("GIT_HEAD");

#[derive(Debug)]
enum Command {
    Other(Vec<OsString>),
    Help,
    Version,
}

fn main() {
    match parse_args().map_err(Some).and_then(run) {
        Ok(_) => process::exit(0),
        Err(err) => {
            if let Some(err) = err {
                term::error(&format!("Error: rad: {}", err));
            }
            process::exit(1);
        }
    }
}

fn parse_args() -> anyhow::Result<Command> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut command = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Long("help") | Short('h') => {
                command = Some(Command::Help);
            }
            Long("version") => {
                command = Some(Command::Version);
            }
            Value(val) if command.is_none() => {
                if val == *"." {
                    command = Some(Command::Other(vec![OsString::from("inspect")]));
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

    Ok(command.unwrap_or_else(|| Command::Other(vec![])))
}

fn print_version() {
    if VERSION.contains("-dev") {
        println!("{} {}+{}", NAME, VERSION, GIT_HEAD)
    } else {
        println!("{} {}", NAME, VERSION)
    }
}

fn print_help() -> anyhow::Result<()> {
    print_version();
    println!("{}", DESCRIPTION);
    println!();

    rad_help::run(Default::default(), term::profile)
}

fn run(command: Command) -> Result<(), Option<anyhow::Error>> {
    match command {
        Command::Version => {
            print_version();
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
                "Authentication",
                rad_auth::run,
                args.to_vec(),
            );
        }
        "checkout" => {
            term::run_command_args::<rad_checkout::Options, _>(
                rad_checkout::HELP,
                "Checkout",
                rad_checkout::run,
                args.to_vec(),
            );
        }
        "help" => {
            term::run_command_args::<rad_help::Options, _>(
                rad_help::HELP,
                "Help",
                rad_help::run,
                args.to_vec(),
            );
        }
        "init" => {
            term::run_command_args::<rad_init::Options, _>(
                rad_init::HELP,
                "Initialization",
                rad_init::run,
                args.to_vec(),
            );
        }
        _ => {
            let exe = format!("{}-{}", NAME, exe);
            let status = process::Command::new(exe.clone()).args(args).status();

            match status {
                Ok(status) => {
                    if !status.success() {
                        return Err(None);
                    }
                }
                Err(err) => {
                    if let ErrorKind::NotFound = err.kind() {
                        return Err(Some(anyhow!("command `{}` not found", exe)));
                    } else {
                        return Err(Some(err.into()));
                    }
                }
            }
        }
    }
    Ok(())
}
