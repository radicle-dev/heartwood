pub mod args;

use radicle::{prelude::*, Node};

use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    for rid in args.rids {
        delete(rid, &mut node, &profile)?;
    }

    Ok(())
}

pub fn delete(rid: RepoId, node: &mut Node, profile: &Profile) -> anyhow::Result<()> {
    if profile.unseed(rid, node)? {
        term::success!("Seeding policy for {} removed", term::format::tertiary(rid));
    }
    Ok(())
}
