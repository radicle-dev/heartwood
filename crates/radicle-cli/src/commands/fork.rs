mod args;

use anyhow::Context as _;

use radicle::rad;

use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let signer = profile.signer()?;
    let storage = &profile.storage;

    let rid = match args.rid {
        Some(rid) => rid,
        None => {
            let (_, rid) = rad::cwd().context("Current directory is not a Radicle repository")?;

            rid
        }
    };

    rad::fork(rid, &signer, &storage)?;
    term::success!("Forked repository {rid} for {}", profile.id());

    Ok(())
}
