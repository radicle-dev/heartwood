mod args;

pub use args::Args;
use args::Command;

use std::path::Path;

use radicle::profile::{config, Config, ConfigPath, RawConfig};

use crate::terminal as term;
use crate::terminal::Element as _;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let home = ctx.home()?;
    let path = home.config();
    let command = args.command.unwrap_or(Command::Show);

    match command {
        Command::Show => {
            let profile = ctx.profile()?;
            term::json::to_pretty(&profile.config, path.as_path())?.print();
        }
        Command::Schema => {
            term::json::to_pretty(&schemars::schema_for!(Config), path.as_path())?.print()
        }
        Command::Get { key } => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let key: ConfigPath = key.into();
            let value = temp_config.get_mut(&key).ok_or_else(|| {
                anyhow::anyhow!("{key} does not exist in configuration found at {path:?}")
            })?;
            print_value(value)?;
        }
        Command::Set { key, value } => {
            let value = modify(path, |tmp| tmp.set(&key.into(), value.into()))?;
            print_value(&value)?;
        }
        Command::Push { key, value } => {
            let value = modify(path, |tmp| tmp.push(&key.into(), value.into()))?;
            print_value(&value)?;
        }
        Command::Remove { key, value } => {
            let value = modify(path, |tmp| tmp.remove(&key.into(), value.into()))?;
            print_value(&value)?;
        }
        Command::Unset { key } => {
            let value = modify(path, |tmp| tmp.unset(&key.into()))?;
            print_value(&value)?;
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
                term::success!("Successfully made changes to the configuration at {path:?}")
            }
            None => term::info!("No changes were made to the configuration at {path:?}"),
        },
    }

    Ok(())
}

fn modify<P, M>(path: P, modification: M) -> anyhow::Result<serde_json::Value>
where
    P: AsRef<Path>,
    M: FnOnce(&mut RawConfig) -> Result<serde_json::Value, config::ModifyError>,
{
    let path = path.as_ref();
    let mut temp_config = RawConfig::from_file(path)?;
    let value = modification(&mut temp_config).map_err(|err| {
        anyhow::anyhow!("failed to modify configuration found at {path:?} due to {err}")
    })?;
    temp_config.write(path)?;
    Ok(value)
}

/// Print a JSON Value.
fn print_value(value: &serde_json::Value) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Null => {}
        serde_json::Value::Bool(b) => term::print(b),
        serde_json::Value::Array(a) => a.iter().try_for_each(print_value)?,
        serde_json::Value::Number(n) => term::print(n),
        serde_json::Value::String(s) => term::print(s),
        serde_json::Value::Object(o) => {
            term::json::to_pretty(&o, Path::new("config.json"))?.print()
        }
    }
    Ok(())
}
