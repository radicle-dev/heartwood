use std::ffi::OsString;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "help",
    description: "CLI help",
    version: env!("RADICLE_VERSION"),
    usage: "Usage: rad help [--help]",
};

const COMMANDS: &[Help] = &[
    crate::commands::auth::HELP,
    crate::commands::block::HELP,
    crate::commands::checkout::HELP,
    crate::commands::clone::HELP,
    crate::commands::config::HELP,
    crate::commands::fork::HELP,
    crate::commands::help::HELP,
    crate::commands::id::HELP,
    crate::commands::init::HELP,
    crate::commands::inbox::HELP,
    crate::commands::inspect::HELP,
    crate::commands::ls::HELP,
    crate::commands::node::HELP,
    crate::commands::patch::HELP,
    crate::commands::path::HELP,
    crate::commands::clean::HELP,
    crate::commands::rad_self::HELP,
    crate::commands::seed::HELP,
    crate::commands::follow::HELP,
    crate::commands::unblock::HELP,
    crate::commands::unfollow::HELP,
    crate::commands::unseed::HELP,
    crate::commands::remote::HELP,
    crate::commands::stats::HELP,
    crate::commands::sync::HELP,
];

#[derive(Default)]
pub struct Options {}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        let mut parser = lexopt::Parser::from_args(args);

        if let Some(arg) = parser.next()? {
            anyhow::bail!(arg.unexpected());
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
                term::print(term::format::yellow(err));
                term::print(term::format::yellow(hint));
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
            term::format::bold(format!("{:-12}", help.name)),
            term::format::dim(help.description)
        );
    }
    term::blank();
    term::print("See `rad <command> --help` to learn about a specific command.");
    term::blank();

    term::print("Do you have feedback?");
    term::print(
        " - Chat <\x1b]8;;https://radicle.zulipchat.com\x1b\\radicle.zulipchat.com\x1b]8;;\x1b\\>",
    );
    term::print(
        " - Mail <\x1b]8;;mailto:feedback@radicle.xyz\x1b\\feedback@radicle.xyz\x1b]8;;\x1b\\>",
    );
    term::print("   (Messages are automatically posted to the public #feedback channel on Zulip.)");

    Ok(())
}
