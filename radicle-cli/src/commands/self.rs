use std::ffi::OsString;

use radicle::crypto::ssh;
use radicle::Profile;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "self",
    description: "Show information about your identity and device",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad self [<option>...]

Options

    --id         Show ID
    --help       Show help
"#,
};

#[derive(Debug)]
enum Show {
    Id,
    All,
}

#[derive(Debug)]
pub struct Options {
    show: Show,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut show: Option<Show> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("id") if show.is_none() => {
                    show = Some(Show::Id);
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                show: show.unwrap_or(Show::All),
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;

    match options.show {
        Show::Id => {
            term::print(profile.id());
        }
        Show::All => all(&profile)?,
    }

    Ok(())
}

fn all(profile: &Profile) -> anyhow::Result<()> {
    let mut table = term::Table::default();

    let node_id = profile.id();
    table.push(["ID", &term::format::tertiary(node_id)]);

    let ssh_short = ssh::fmt::fingerprint(node_id);
    table.push(["Key (hash)", &term::format::tertiary(ssh_short)]);

    let ssh_long = ssh::fmt::key(node_id);
    table.push(["Key (full)", &term::format::tertiary(ssh_long)]);

    let storage_path = profile.home.storage();
    table.push([
        "Storage (git)",
        &term::format::tertiary(storage_path.display()),
    ]);

    let keys_path = profile.home.keys();
    table.push([
        "Storage (keys)",
        &term::format::tertiary(keys_path.display()),
    ]);

    let node_path = profile.home.node();
    table.push([
        "Node (socket)",
        &term::format::tertiary(node_path.join("radicle.sock").display()),
    ]);

    table.render();

    Ok(())
}
