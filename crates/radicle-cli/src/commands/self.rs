use std::ffi::OsString;

use radicle::crypto::ssh;
use radicle::node::Handle as _;
use radicle::{Node, Profile};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "self",
    description: "Show information about your identity and device",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad self [<option>...]

Options

    --did                Show your DID
    --alias              Show your Node alias
    --home               Show your Radicle home
    --config             Show the location of your configuration file
    --ssh-key            Show your public key in OpenSSH format
    --ssh-fingerprint    Show your public key fingerprint in OpenSSH format
    --help               Show help
"#,
};

#[derive(Debug)]
enum Show {
    Alias,
    NodeId,
    Did,
    Home,
    Config,
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
                Long("alias") if show.is_none() => {
                    show = Some(Show::Alias);
                }
                Long("nid") if show.is_none() => {
                    show = Some(Show::NodeId);
                }
                Long("did") if show.is_none() => {
                    show = Some(Show::Did);
                }
                Long("home") if show.is_none() => {
                    show = Some(Show::Home);
                }
                Long("config") if show.is_none() => {
                    show = Some(Show::Config);
                }
                Long("ssh-key") if show.is_none() => {
                    show = Some(Show::SshKey);
                }
                Long("ssh-fingerprint") if show.is_none() => {
                    show = Some(Show::SshFingerprint);
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => anyhow::bail!(arg.unexpected()),
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
        Show::Alias => {
            term::print(profile.config.alias());
        }
        Show::NodeId => {
            term::warning(
                "The option `--nid` is deprecated, please use `rad node status --only nid` instead.",
            );
            term::print(
                Node::new(profile.socket())
                    .nid()
                    .ok()
                    .unwrap_or_else(|| *profile.id()),
            );
        }
        Show::Did => {
            term::print(profile.did());
        }
        Show::Home => {
            term::print(profile.home().path().display());
        }
        Show::Config => {
            term::print(profile.home.config().display());
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
    let mut table = term::Table::<2, term::Label>::default();

    table.push([
        term::format::style("Alias").into(),
        term::format::primary(profile.config.alias()).into(),
    ]);

    let did = profile.did();
    table.push([
        term::format::style("DID").into(),
        term::format::tertiary(did).into(),
    ]);

    let socket = profile.socket();
    let node = if Node::new(&socket).is_running() {
        term::format::positive(format!("running ({})", socket.display()))
    } else {
        term::format::negative("not running".to_string())
    };
    table.push([term::format::style("Node").into(), node.to_string().into()]);

    let ssh_agent = match ssh::agent::Agent::connect() {
        Ok(c) => term::format::positive(format!(
            "running ({})",
            c.path()
                .map(|p| p.display().to_string())
                .unwrap_or(String::from("?"))
        )),
        Err(e) if e.is_not_running() => term::format::yellow(String::from("not running")),
        Err(e) => term::format::negative(format!("error: {e}")),
    };
    table.push([
        term::format::style("SSH").into(),
        ssh_agent.to_string().into(),
    ]);

    let id = profile.id();
    let ssh_short = ssh::fmt::fingerprint(id);
    table.push([
        term::format::style("├╴Key (hash)").into(),
        term::format::tertiary(ssh_short).into(),
    ]);

    let ssh_long = ssh::fmt::key(id);
    table.push([
        term::format::style("└╴Key (full)").into(),
        term::format::tertiary(ssh_long).into(),
    ]);

    let home = profile.home();
    table.push([
        term::format::style("Home").into(),
        term::format::tertiary(home.path().display()).into(),
    ]);

    let config_path = profile.home.config();
    table.push([
        term::format::style("├╴Config").into(),
        term::format::tertiary(config_path.display()).into(),
    ]);

    let storage_path = profile.home.storage();
    table.push([
        term::format::style("├╴Storage").into(),
        term::format::tertiary(storage_path.display()).into(),
    ]);

    let keys_path = profile.home.keys();
    table.push([
        term::format::style("├╴Keys").into(),
        term::format::tertiary(keys_path.display()).into(),
    ]);

    table.push([
        term::format::style("└╴Node").into(),
        term::format::tertiary(profile.home.node().display()).into(),
    ]);

    table.print();

    Ok(())
}
