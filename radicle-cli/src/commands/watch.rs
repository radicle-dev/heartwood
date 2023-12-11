use std::ffi::OsString;
use std::{thread, time};

use anyhow::{anyhow, Context as _};

use radicle::git;
use radicle::prelude::{Id, NodeId};
use radicle::storage::{ReadRepository, ReadStorage};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "wait",
    description: "Wait for some state to be updated",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad watch -r <ref> [-t <oid>] [--repo <rid>] [<option>...]

    Watches a Git reference, and optionally exits when it reaches a target value.
    If no target value is passed, exits when the target changes.

Options

        --repo      <rid>       The repository to watch (default: `rad .`)
        --node      <nid>       The namespace under which this reference exists
                                (default: `rad self --nid`)
    -r, --ref       <ref>       The fully-qualified Git reference (branch, tag, etc.) to watch,
                                eg. 'refs/heads/master'
    -t, --target    <oid>       The target OID (commit hash) that when reached,
                                will cause the command to exit
    -i, --interval  <millis>    How often, in milliseconds, to check the reference target
                                (default: 1000)
    -h, --help                  Print help
"#,
};

pub struct Options {
    rid: Option<Id>,
    refstr: git::RefString,
    target: Option<git::Oid>,
    nid: Option<NodeId>,
    interval: time::Duration,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid = None;
        let mut nid: Option<NodeId> = None;
        let mut target: Option<git::Oid> = None;
        let mut refstr: Option<git::RefString> = None;
        let mut interval: Option<time::Duration> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("repo") => {
                    let value = parser.value()?;
                    let value = term::args::rid(&value)?;

                    rid = Some(value);
                }
                Long("node") => {
                    let value = parser.value()?;
                    let value = term::args::nid(&value)?;

                    nid = Some(value);
                }
                Long("ref") | Short('r') => {
                    let value = parser.value()?;
                    let value = term::args::refstring("ref", value)?;

                    refstr = Some(value);
                }
                Long("target") | Short('t') => {
                    let value = parser.value()?;
                    let value = term::args::oid(&value)?;

                    target = Some(value);
                }
                Long("interval") | Short('i') => {
                    let value = parser.value()?;
                    let value = term::args::milliseconds(&value)?;

                    interval = Some(value);
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                rid,
                refstr: refstr.ok_or_else(|| anyhow!("a reference must be provided"))?,
                nid,
                target,
                interval: interval.unwrap_or(time::Duration::from_secs(1)),
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let qualified = options
        .refstr
        .qualified()
        .ok_or_else(|| anyhow!("reference must be fully-qualified, eg. 'refs/heads/master'"))?;
    let nid = options.nid.unwrap_or(profile.public_key);
    let rid = match options.rid {
        Some(rid) => rid,
        None => {
            let (_, rid) =
                radicle::rad::cwd().context("Current directory is not a radicle project")?;
            rid
        }
    };
    let repo = storage.repository(rid)?;

    if let Some(target) = options.target {
        while reference(&repo, &nid, &qualified)? != Some(target) {
            thread::sleep(options.interval);
        }
    } else {
        let initial = reference(&repo, &nid, &qualified)?;

        loop {
            thread::sleep(options.interval);
            let oid = reference(&repo, &nid, &qualified)?;
            if oid != initial {
                term::info!("{}", oid.unwrap_or(git::raw::Oid::zero().into()));
                break;
            }
        }
    }
    Ok(())
}

fn reference<R: ReadRepository>(
    repo: &R,
    nid: &NodeId,
    qual: &git::Qualified,
) -> Result<Option<git::Oid>, git::raw::Error> {
    match repo.reference_oid(nid, qual) {
        Ok(oid) => Ok(Some(oid)),
        Err(e) if git::ext::is_not_found_err(&e) => Ok(None),
        Err(e) => Err(e),
    }
}
