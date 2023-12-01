use std::ffi::OsString;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use super::*;

pub const HELP: Help = Help {
    name: "help",
    description: "CLI help",
    version: env!("CARGO_PKG_VERSION"),
    usage: "Usage: rad help [--help]",
};

const COMMANDS: &[Help] = &[
    rad_auth::HELP,
    rad_checkout::HELP,
    rad_clone::HELP,
    rad_fork::HELP,
    rad_help::HELP,
    rad_id::HELP,
    rad_init::HELP,
    rad_inspect::HELP,
    rad_issue::HELP,
    rad_ls::HELP,
    rad_node::HELP,
    rad_patch::HELP,
    rad_path::HELP,
    rad_clean::HELP,
    rad_self::HELP,
    rad_track::HELP,
    rad_untrack::HELP,
    rad_remote::HELP,
    rad_sync::HELP,
];

#[derive(Default)]
pub struct Options {}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        let mut parser = lexopt::Parser::from_args(args);

        if let Some(arg) = parser.next()? {
            return Err(anyhow::anyhow!(arg.unexpected()));
        }
        Err(Error::HelpManual { name: "rad" }.into())
    }
}

pub fn run(_options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    println!("Usage: rad <command> [--help]");

    if ctx.profile().is_err() {
        println!();
        println!(
            "{}",
            term::format::highlight("It looks like this is your first time using radicle.")
        );
        println!(
            "{}",
            term::format::highlight("To get started, use `rad auth` to authenticate.")
        );
        println!();
    }

    println!("Common `rad` commands used in various situations:");
    println!();

    for help in COMMANDS {
        println!(
            "\t{} {}",
            term::format::bold(format!("{:-12}", help.name)),
            term::format::dim(help.description)
        );
    }
    println!();
    println!("See `rad <command> --help` to learn about a specific command.");
    println!();

    Ok(())
}
