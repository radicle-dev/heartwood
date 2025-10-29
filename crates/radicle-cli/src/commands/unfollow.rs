mod args;

use radicle::node::Handle;

use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());
    let nid = args.nid;

    let unfollowed = match node.unfollow(nid) {
        Ok(updated) => updated,
        Err(e) if e.is_connection_err() => {
            let mut config = profile.policies_mut()?;
            config.unfollow(&nid)?
        }
        Err(e) => return Err(e.into()),
    };
    if unfollowed {
        term::success!("Follow policy for {} removed", term::format::tertiary(nid),);
    }
    Ok(())
}
