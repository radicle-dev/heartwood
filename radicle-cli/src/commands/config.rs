#![allow(clippy::or_fun_call)]
use std::ffi::OsString;

use anyhow::anyhow;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "config",
    description: "Manage your local radicle configuration",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad config [<option>...]

    If no argument is specified, prints the current radicle configuration as JSON.

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
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        Ok((Options {}, vec![]))
    }
}

pub fn run(_options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let path = profile.home.config();
    let output = term::json::to_pretty(&profile.config, path.as_path())?;

    output.print();

    Ok(())
}
