mod args;

use radicle::node::policy::Policy;

use crate::terminal as term;

use term::args::BlockTarget;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut policies = profile.policies_mut()?;

    let updated = match args.target {
        BlockTarget::Node(nid) => policies.set_follow_policy(&nid, Policy::Block)?,
        BlockTarget::Repo(rid) => policies.set_seed_policy(&rid, Policy::Block)?,
    };
    if updated {
        term::success!("Policy for {} set to 'block'", args.target);
    }
    Ok(())
}
