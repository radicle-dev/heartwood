mod args;

pub use args::Args;
use args::Command;

use std::path::Path;

use radicle::profile::{Config, config};

#[allow(deprecated)]
use radicle::profile::config::{ConfigPath, RawConfig};

use crate::terminal::Element as _;
use crate::{terminal as term, warning};

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
        #[allow(deprecated)]
        Command::Get { key } => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let key: ConfigPath = key.into();
            let value = temp_config.get_mut(&key).ok_or_else(|| {
                anyhow::anyhow!("{key} does not exist in configuration found at {path:?}")
            })?;
            print_value(value)?;
        }
        #[allow(deprecated)]
        Command::Set { key, value } => {
            warning::obsolete("rad config set");
            let value = modify(path, |tmp| tmp.set(&key.into(), value.into()))?;
            print_value(&value)?;
        }
        #[allow(deprecated)]
        Command::Push { key, value } => {
            warning::obsolete("rad config push");
            let value = modify(path, |tmp| tmp.push(&key.into(), value.into()))?;
            print_value(&value)?;
        }
        #[allow(deprecated)]
        Command::Remove { key, value } => {
            warning::obsolete("rad config remove");
            let value = modify(path, |tmp| tmp.remove(&key.into(), value.into()))?;
            print_value(&value)?;
        }
        #[allow(deprecated)]
        Command::Unset { key } => {
            warning::obsolete("rad config unset");
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

#[deprecated]
#[allow(deprecated)]
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
#[deprecated]
#[allow(deprecated)]
fn print_value(value: &serde_json::Value) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Null => {}
        serde_json::Value::Bool(b) => term::println(b),
        serde_json::Value::Array(a) => a.iter().try_for_each(print_value)?,
        serde_json::Value::Number(n) => term::println(n),
        serde_json::Value::String(s) => term::println(s),
        serde_json::Value::Object(o) => {
            term::json::to_pretty(&o, Path::new("config.json"))?.print()
        }
    }
    Ok(())
}
