mod args;

use std::{thread, time};

use anyhow::{anyhow, Context as _};

use radicle::git;
use radicle::git::raw::ErrorExt as _;
use radicle::prelude::NodeId;
use radicle::storage::{ReadRepository, ReadStorage};

use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let qualified = args
        .refstr
        .qualified()
        .ok_or_else(|| anyhow!("reference must be fully-qualified, eg. 'refs/heads/master'"))?;
    let nid = args.node.unwrap_or(profile.public_key);
    let rid = match args.repo {
        Some(rid) => rid,
        None => {
            let (_, rid) =
                radicle::rad::cwd().context("Current directory is not a Radicle repository")?;
            rid
        }
    };
    let repo = storage.repository(rid)?;
    let now = time::SystemTime::now();
    let timeout = args.timeout();
    let interval = args.interval();

    if let Some(target) = args.target {
        while reference(&repo, &nid, &qualified)? != Some(target) {
            thread::sleep(interval);
            if now.elapsed()? >= timeout {
                anyhow::bail!("timed out after {}ms", timeout.as_millis());
            }
        }
    } else {
        let initial = reference(&repo, &nid, &qualified)?;

        loop {
            thread::sleep(interval);
            let oid = reference(&repo, &nid, &qualified)?;
            if oid != initial {
                term::info!("{}", oid.unwrap_or(git::raw::Oid::zero().into()));
                break;
            }
            if now.elapsed()? >= timeout {
                anyhow::bail!("timed out after {}ms", timeout.as_millis());
            }
        }
    }
    Ok(())
}

fn reference<R: ReadRepository>(
    repo: &R,
    nid: &NodeId,
    qual: &git::fmt::Qualified,
) -> Result<Option<git::Oid>, git::raw::Error> {
    match repo.reference_oid(nid, qual) {
        Ok(oid) => Ok(Some(oid)),
        Err(e) if e.is_not_found() => Ok(None),
        Err(e) => Err(e),
    }
}
