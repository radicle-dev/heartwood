use std::ffi::OsString;

use crate::terminal as term;
use crate::terminal::display;
use crate::terminal::args::{Args, Error, Help};

use super::*;

pub const HELP: Help = Help {
    name: "help",
    description: "CLI help",
    version: env!("RADICLE_VERSION"),
    usage: "Usage: rad help [--help]",
};

const COMMANDS: &[Help] = &[
    rad_auth::HELP,
    rad_block::HELP,
    rad_checkout::HELP,
    rad_clone::HELP,
    rad_config::HELP,
    rad_fork::HELP,
    rad_help::HELP,
    rad_id::HELP,
    rad_init::HELP,
    rad_inbox::HELP,
    rad_inspect::HELP,
    rad_issue::HELP,
    rad_ls::HELP,
    rad_node::HELP,
    rad_patch::HELP,
    rad_path::HELP,
    rad_clean::HELP,
    rad_self::HELP,
    rad_seed::HELP,
    rad_follow::HELP,
    rad_unblock::HELP,
    rad_unfollow::HELP,
    rad_unseed::HELP,
    rad_remote::HELP,
    rad_stats::HELP,
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
    term::print("Usage: rad <command> [--help]");

    if let Err(e) = ctx.profile() {
        term::blank();
        match e.downcast_ref() {
            Some(term::args::Error::WithHint { err, hint }) => {
                term::print_display(&term::format::yellow(err));
                term::print_display(&term::format::yellow(hint));
            }
            Some(e) => {
                term::error(e);
            }
            None => {
                term::error(e);
            }
        }
        term::blank();
    }

    term::print("Common `rad` commands used in various situations:");
    term::blank();

    for help in COMMANDS {
        term::info!(
            "\t{} {}",
            display(&term::format::bold(format!("{:-12}", help.name))),
            display(&term::format::dim(help.description))
        );
    }
    term::blank();
    term::print("See `rad <command> --help` to learn about a specific command.");
    term::blank();

    Ok(())
}
