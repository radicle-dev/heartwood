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
    CommandItem::Clap {
        name: "auth",
        about: crate::commands::auth::ABOUT,
    },
    CommandItem::Clap {
        name: "block",
        about: crate::commands::block::ABOUT,
    },
    CommandItem::Clap {
        name: "checkout",
        about: crate::commands::checkout::ABOUT,
    },
    CommandItem::Clap {
        name: "clone",
        about: crate::commands::clone::ABOUT,
    },
    CommandItem::Clap {
        name: "config",
        about: crate::commands::config::ABOUT,
    },
    CommandItem::Clap {
        name: "debug",
        about: crate::commands::debug::ABOUT,
    },
    CommandItem::Clap {
        name: "fork",
        about: crate::commands::fork::ABOUT,
    },
    CommandItem::Lexopt(crate::commands::help::HELP),
    CommandItem::Clap {
        name: "id",
        about: crate::commands::id::ABOUT,
    },
    CommandItem::Clap {
        name: "init",
        about: crate::commands::init::ABOUT,
    },
    CommandItem::Clap {
        name: "inbox",
        about: crate::commands::inbox::ABOUT,
    },
    CommandItem::Clap {
        name: "inspect",
        about: crate::commands::inspect::ABOUT,
    },
    CommandItem::Clap {
        name: "issue",
        about: crate::commands::issue::ABOUT,
    },
    CommandItem::Clap {
        name: "ls",
        about: crate::commands::ls::ABOUT,
    },
    CommandItem::Clap {
        name: "node",
        about: crate::commands::node::ABOUT,
    },
    CommandItem::Clap {
        name: "patch",
        about: crate::commands::patch::ABOUT,
    },
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
    CommandItem::Clap {
        name: "self",
        about: crate::commands::rad_self::ABOUT,
    },
    CommandItem::Clap {
        name: "seed",
        about: crate::commands::seed::ABOUT,
    },
    CommandItem::Clap {
        name: "follow",
        about: crate::commands::follow::ABOUT,
    },
    CommandItem::Clap {
        name: "unblock",
        about: crate::commands::unblock::ABOUT,
    },
    CommandItem::Clap {
        name: "unfollow",
        about: crate::commands::unfollow::ABOUT,
    },
    CommandItem::Clap {
        name: "unseed",
        about: crate::commands::unseed::ABOUT,
    },
    CommandItem::Clap {
        name: "remote",
        about: crate::commands::remote::ABOUT,
    },
    CommandItem::Clap {
        name: "stats",
        about: crate::commands::stats::ABOUT,
    },
    CommandItem::Clap {
        name: "sync",
        about: crate::commands::sync::ABOUT,
    },
    CommandItem::Clap {
        name: "watch",
        about: crate::commands::watch::ABOUT,
    },
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
