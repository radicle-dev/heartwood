#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::process;

use anyhow::anyhow;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "config",
    description: "Manage your local Radicle configuration",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad config [<option>...]
    rad config show
    rad config init
    rad config edit

    If no argument is specified, prints the current radicle configuration as JSON.
    To initialize a new configuration file, use `rad config init`.

Options

    --help    Print help

"#,
};

#[derive(Default)]
enum Operation {
    #[default]
    Show,
    Init,
    Edit,
}

pub struct Options {
    op: Operation,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<Operation> = None;

        #[allow(clippy::never_loop)]
        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "show" => op = Some(Operation::Show),
                    "edit" => op = Some(Operation::Edit),
                    "init" => op = Some(Operation::Init),
                    unknown => anyhow::bail!("unknown operation '{unknown}'"),
                },
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                op: op.unwrap_or_default(),
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let path = profile.home.config();

    match options.op {
        Operation::Show => {
            term::json::to_pretty(&profile.config, path.as_path())?.print();
        }
        Operation::Init => {
            if path.try_exists()? {
                anyhow::bail!("configuration file already exists at `{}`", path.display());
            }
            profile.config.write(&path)?;
        }
        Operation::Edit => {
            let Some(cmd) = term::editor::default_editor() else {
                anyhow::bail!("no editor configured; please set the `EDITOR` environment variable");
            };
            process::Command::new(cmd)
                .stdout(process::Stdio::inherit())
                .stderr(process::Stdio::inherit())
                .stdin(process::Stdio::inherit())
                .arg(&path)
                .spawn()?
                .wait()?;
        }
    }

    Ok(())
}
