mod args;

pub use args::Args;
use args::Command;

use radicle::profile::Config;

use crate::terminal as term;
use crate::terminal::Element as _;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let path = ctx.home()?.config();
    let command = args.command.unwrap_or(Command::Show);

    match command {
        Command::Show => {
            let profile = ctx.profile()?;
            term::json::to_pretty(&profile.config, path.as_path())?.print();
        }
        Command::Schema => {
            term::json::to_pretty(&schemars::schema_for!(Config), path.as_path())?.print()
        }
        Command::Init { alias } => {
            if path.try_exists()? {
                anyhow::bail!("configuration file already exists at `{}`", path.display());
            }
            Config::init(alias, &path)?;
            term::success!(
                "Initialized new Radicle configuration at {}",
                path.display()
            );
        }
        Command::Edit => match term::editor::Editor::new(&path)?.extension("json").edit()? {
            Some(_) => {
                term::success!(
                    "Successfully made changes to the configuration at {}",
                    path.display()
                )
            }
            None => term::info!(
                "No changes were made to the configuration at {}",
                path.display()
            ),
        },
    }

    Ok(())
}
