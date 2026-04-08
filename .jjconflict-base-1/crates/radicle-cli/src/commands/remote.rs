//! Remote Command implementation

pub mod add;
pub mod list;
pub mod rm;

mod args;

use anyhow::anyhow;

use radicle::storage::ReadStorage;

use crate::terminal as term;
use crate::terminal::Context;

pub use args::Args;
use args::{Command, ListOption};

pub fn run(args: Args, ctx: impl Context) -> anyhow::Result<()> {
    let (working, rid) = radicle::rad::cwd()
        .map_err(|_| anyhow!("this command must be run in the context of a repository"))?;
    let profile = ctx.profile()?;
    let command = args
        .command
        .unwrap_or_else(|| Command::List(args.empty.into()));
    match command {
        Command::Add {
            nid,
            name,
            fetch,
            sync,
        } => {
            let proj = profile.storage.repository(rid)?.project()?;
            let branch = proj.default_branch();
            self::add::run(
                rid,
                &nid,
                name,
                Some(branch.clone()),
                &profile,
                &working,
                fetch.should_fetch(),
                sync.should_sync(),
            )?
        }
        Command::Rm { ref name } => self::rm::run(name, &working)?,
        Command::List(args) => match ListOption::from(args) {
            ListOption::All => {
                let tracked = list::tracked(&working)?;
                let untracked = list::untracked(rid, &profile, tracked.iter())?;
                // Only include a blank line if we're printing both tracked and untracked
                let include_blank_line = !tracked.is_empty() && !untracked.is_empty();

                list::print_tracked(tracked.iter());
                if include_blank_line {
                    term::blank();
                }
                list::print_untracked(untracked.iter());
            }
            ListOption::Tracked => {
                let tracked = list::tracked(&working)?;
                list::print_tracked(tracked.iter());
            }
            ListOption::Untracked => {
                let tracked = list::tracked(&working)?;
                let untracked = list::untracked(rid, &profile, tracked.iter())?;
                list::print_untracked(untracked.iter());
            }
        },
    };
    Ok(())
}
