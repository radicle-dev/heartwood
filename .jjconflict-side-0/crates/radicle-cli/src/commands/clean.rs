mod args;

use radicle::storage;
use radicle::storage::WriteStorage;

use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let rid = args.repo;
    let path = storage::git::paths::repository(storage, &rid);

    if !path.exists() {
        anyhow::bail!("repository {rid} was not found");
    }

    if args.no_confirm || term::confirm(format!("Clean {rid}?")) {
        let cleaned = storage.clean(rid)?;
        for remote in cleaned {
            term::info!("Removed {remote}");
        }
        term::success!("Successfully cleaned {rid}");
    }

    Ok(())
}
