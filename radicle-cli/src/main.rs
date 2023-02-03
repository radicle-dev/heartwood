use std::ffi::OsString;
use std::{io::ErrorKind, iter, process};

use anyhow::anyhow;

use radicle_cli::commands::*;
use radicle_cli::terminal as term;

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
                term::error(format!("Error: rad: {err}"));
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
        println!("{NAME} {VERSION}+{GIT_HEAD}")
    } else {
        println!("{NAME} {VERSION}")
    }
}

fn print_help() -> anyhow::Result<()> {
    print_version();
    println!("{DESCRIPTION}");
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
        "assign" => {
            term::run_command_args::<rad_assign::Options, _>(
                rad_assign::HELP,
                "Assign",
                rad_assign::run,
                args.to_vec(),
            );
        }
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
        "clone" => {
            term::run_command_args::<rad_clone::Options, _>(
                rad_clone::HELP,
                "Clone",
                rad_clone::run,
                args.to_vec(),
            );
        }
        "comment" => {
            term::run_command_args::<rad_comment::Options, _>(
                rad_comment::HELP,
                "Comment",
                rad_comment::run,
                args.to_vec(),
            );
        }
        "delegate" => {
            term::run_command_args::<rad_delegate::Options, _>(
                rad_delegate::HELP,
                "Delegate",
                rad_delegate::run,
                args.to_vec(),
            );
        }
        "edit" => {
            term::run_command_args::<rad_edit::Options, _>(
                rad_edit::HELP,
                "Edit",
                rad_edit::run,
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
        "inspect" => {
            term::run_command_args::<rad_inspect::Options, _>(
                rad_inspect::HELP,
                "Inspect",
                rad_inspect::run,
                args.to_vec(),
            );
        }
        "issue" => {
            term::run_command_args::<rad_issue::Options, _>(
                rad_issue::HELP,
                "Command",
                rad_issue::run,
                args.to_vec(),
            );
        }
        "ls" => {
            term::run_command_args::<rad_ls::Options, _>(
                rad_ls::HELP,
                "List",
                rad_ls::run,
                args.to_vec(),
            );
        }
        "merge" => {
            term::run_command_args::<rad_merge::Options, _>(
                rad_merge::HELP,
                "Merge",
                rad_merge::run,
                args.to_vec(),
            );
        }
        "patch" => {
            term::run_command_args::<rad_patch::Options, _>(
                rad_patch::HELP,
                "Command",
                rad_patch::run,
                args.to_vec(),
            );
        }
        "path" => {
            term::run_command_args::<rad_path::Options, _>(
                rad_path::HELP,
                "Path",
                rad_path::run,
                args.to_vec(),
            );
        }
        "push" => {
            term::run_command_args::<rad_push::Options, _>(
                rad_push::HELP,
                "Push",
                rad_push::run,
                args.to_vec(),
            );
        }
        "review" => {
            term::run_command_args::<rad_review::Options, _>(
                rad_review::HELP,
                "Review",
                rad_review::run,
                args.to_vec(),
            );
        }
        "rm" => {
            term::run_command_args::<rad_rm::Options, _>(
                rad_rm::HELP,
                "Remove",
                rad_rm::run,
                args.to_vec(),
            );
        }
        "self" => {
            term::run_command_args::<rad_self::Options, _>(
                rad_self::HELP,
                "Command",
                rad_self::run,
                args.to_vec(),
            );
        }
        "track" => {
            term::run_command_args::<rad_track::Options, _>(
                rad_track::HELP,
                "Track",
                rad_track::run,
                args.to_vec(),
            );
        }
        "unassign" => {
            term::run_command_args::<rad_unassign::Options, _>(
                rad_unassign::HELP,
                "Unassign",
                rad_unassign::run,
                args.to_vec(),
            );
        }
        "untrack" => {
            term::run_command_args::<rad_untrack::Options, _>(
                rad_untrack::HELP,
                "Untrack",
                rad_untrack::run,
                args.to_vec(),
            );
        }
        "web" => term::run_command_args::<rad_web::Options, _>(
            rad_web::HELP,
            "Web",
            rad_web::run,
            args.to_vec(),
        ),
        _ => {
            let exe = format!("{NAME}-{exe}");
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
