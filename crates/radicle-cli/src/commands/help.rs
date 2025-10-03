use std::ffi::OsString;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "help",
    description: "CLI help",
    version: env!("RADICLE_VERSION"),
    usage: "Usage: rad help [--help]",
};

enum CommandItem {
    Lexopt(Help),
    Clap {
        name: &'static str,
        about: &'static str,
    },
}

impl CommandItem {
    fn name(&self) -> &str {
        match self {
            CommandItem::Lexopt(help) => help.name,
            CommandItem::Clap { name, .. } => name,
        }
    }

    fn description(&self) -> &str {
        match self {
            CommandItem::Lexopt(help) => help.description,
            CommandItem::Clap {
                about: description, ..
            } => description,
        }
    }
}

const COMMANDS: &[CommandItem] = &[
    CommandItem::Lexopt(crate::commands::auth::HELP),
    CommandItem::Clap {
        name: "block",
        about: crate::commands::block::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::checkout::HELP),
    CommandItem::Clap {
        name: "clone",
        about: crate::commands::clone::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::config::HELP),
    CommandItem::Clap {
        name: "debug",
        about: crate::commands::debug::ABOUT,
    },
    CommandItem::Clap {
        name: "fork",
        about: crate::commands::fork::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::help::HELP),
    CommandItem::Lexopt(crate::commands::id::HELP),
    CommandItem::Clap {
        name: "init",
        about: crate::commands::init::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::inbox::HELP),
    CommandItem::Lexopt(crate::commands::inspect::HELP),
    CommandItem::Clap {
        name: "issue",
        about: crate::commands::issue::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::ls::HELP),
    CommandItem::Lexopt(crate::commands::node::HELP),
    CommandItem::Lexopt(crate::commands::patch::HELP),
    CommandItem::Clap {
        name: "path",
        about: crate::commands::path::ABOUT,
    },
    CommandItem::Clap {
        name: "publish",
        about: crate::commands::publish::ABOUT,
    },
    CommandItem::Clap {
        name: "clean",
        about: crate::commands::clean::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::rad_self::HELP),
    CommandItem::Lexopt(crate::commands::seed::HELP),
    CommandItem::Lexopt(crate::commands::follow::HELP),
    CommandItem::Lexopt(crate::commands::unblock::HELP),
    CommandItem::Clap {
        name: "unfollow",
        about: crate::commands::unfollow::ABOUT,
    },
    CommandItem::Clap {
        name: "unseed",
        about: crate::commands::unseed::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::remote::HELP),
    CommandItem::Clap {
        name: "stats",
        about: crate::commands::stats::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::sync::HELP),
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
            term::format::bold(format!("{:-12}", help.name())),
            term::format::dim(help.description())
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
