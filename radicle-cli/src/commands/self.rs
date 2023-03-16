use std::ffi::OsString;

use radicle::crypto::ssh;
use radicle::Profile;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "self",
    description: "Show information about your identity and device",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad self [<option>...]

Options

    --nid                Show your Node ID (NID)
    --did                Show your DID
    --ssh-key            Show your public key in OpenSSH format
    --ssh-fingerprint    Show your public key fingerprint in OpenSSH format
    --help               Show help
"#,
};

#[derive(Debug)]
enum Show {
    NodeId,
    Did,
    SshKey,
    SshFingerprint,
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
                Long("nid") if show.is_none() => {
                    show = Some(Show::NodeId);
                }
                Long("did") if show.is_none() => {
                    show = Some(Show::Did);
                }
                Long("ssh-key") if show.is_none() => {
                    show = Some(Show::SshKey);
                }
                Long("ssh-fingerprint") if show.is_none() => {
                    show = Some(Show::SshFingerprint);
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
        Show::NodeId => {
            term::print(profile.id());
        }
        Show::Did => {
            term::print(profile.did());
        }
        Show::SshKey => {
            term::print(ssh::fmt::key(profile.id()));
        }
        Show::SshFingerprint => {
            term::print(ssh::fmt::fingerprint(profile.id()));
        }
        Show::All => all(&profile)?,
    }

    Ok(())
}

fn all(profile: &Profile) -> anyhow::Result<()> {
    let mut table = term::Table::default();

    let did = profile.did();
    table.push([
        term::format::style("DID").to_string(),
        term::format::tertiary(did).to_string(),
    ]);

    let node_id = profile.id();
    table.push([
        term::format::style("Node ID (NID)").to_string(),
        term::format::tertiary(node_id).to_string(),
    ]);

    let ssh_short = ssh::fmt::fingerprint(node_id);
    table.push([
        term::format::style("Key (hash)").to_string(),
        term::format::tertiary(ssh_short).to_string(),
    ]);

    let ssh_long = ssh::fmt::key(node_id);
    table.push([
        term::format::style("Key (full)").to_string(),
        term::format::tertiary(ssh_long).to_string(),
    ]);

    let storage_path = profile.home.storage();
    table.push([
        term::format::style("Storage (git)").to_string(),
        term::format::tertiary(storage_path.display()).to_string(),
    ]);

    let keys_path = profile.home.keys();
    table.push([
        term::format::style("Storage (keys)").to_string(),
        term::format::tertiary(keys_path.display()).to_string(),
    ]);

    let socket_path = profile.socket();
    table.push([
        term::format::style("Node (socket)").to_string(),
        term::format::tertiary(socket_path.display()).to_string(),
    ]);

    table.print();

    Ok(())
}
