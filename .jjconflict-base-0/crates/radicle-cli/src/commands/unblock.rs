mod args;

use crate::terminal as term;

use term::args::BlockTarget;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut policies = profile.policies_mut()?;

    let updated = match args.target {
        BlockTarget::Node(nid) => policies.unblock_nid(&nid)?,
        BlockTarget::Repo(rid) => policies.unblock_rid(&rid)?,
    };

    if updated {
        term::success!("The 'block' policy for {} is removed", args.target);
    } else {
        term::info!("No 'block' policy exists for {}", args.target)
    }
    Ok(())
}
