mod args;

use radicle::storage::WriteStorage;

use crate::terminal as term;
use crate::terminal::args::rid_or_cwd;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let (_, rid) = rid_or_cwd(args.repo)?;

    if args.no_confirm || term::confirm(format!("Clean {rid}?")) {
        let cleaned = storage.clean(rid)?;
        for remote in cleaned {
            term::info!("Removed {remote}");
        }
        term::success!("Successfully cleaned {rid}");
    }

    Ok(())
}
