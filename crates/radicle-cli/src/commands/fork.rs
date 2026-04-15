mod args;

use radicle::rad;

use crate::{
    terminal::{self as term, args::rid_or_cwd},
    warning,
};

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    warning::deprecated("rad fork", "git push");
    let profile = ctx.profile()?;
    let signer = profile.signer()?;
    let storage = &profile.storage;
    let (_, rid) = rid_or_cwd(args.repo)?;

    rad::fork(rid, &signer, &storage)?;
    term::success!("Forked repository {rid} for {}", profile.id());

    Ok(())
}
