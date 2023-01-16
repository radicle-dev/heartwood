#![allow(clippy::or_fun_call)]
use std::ffi::OsString;

use anyhow::anyhow;
use radicle::profile;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "path",
    description: "Display the radicle home path",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad path [--help]

    If no argument is specified, the radicle "home" path is displayed.

Options

    --help    Print help

"#,
};

pub struct Options {}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);

        #[allow(clippy::never_loop)]
        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        Ok((Options {}, vec![]))
    }
}

pub fn run(_options: Options, _ctx: impl term::Context) -> anyhow::Result<()> {
    let home = profile::home()?;

    println!("{}", home.path().display());

    Ok(())
}
