mod args;

use radicle::profile;

use crate::terminal as term;

pub use args::Args;

pub fn run(_args: Args, _ctx: impl term::Context) -> anyhow::Result<()> {
    let home = profile::home()?;

    println!("{}", home.path().display());

    Ok(())
}
