#![allow(clippy::or_fun_call)]
use std::ffi::OsString;
use std::path::Path;
use std::str::FromStr;

use anyhow::anyhow;
use radicle::node::Alias;
use radicle::profile::{Config, ConfigError, ConfigPath, RawConfig};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Element as _;

pub const HELP: Help = Help {
    name: "config",
    description: "Manage your local Radicle configuration",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad config [<option>...]
    rad config show [<option>...]
    rad config init --alias <alias> [<option>...]
    rad config edit [<option>...]
    rad config get <key> [<option>...]
    rad config schema [<option>...]
    rad config set <key> <value> [<option>...]
    rad config unset <key> [<option>...]
    rad config push <key> <value> [<option>...]
    rad config remove <key> <value> [<option>...]

    If no argument is specified, prints the current radicle configuration as JSON.
    To initialize a new configuration file, use `rad config init`.

Options

    --help    Print help

"#,
};

#[derive(Default)]
enum Operation {
    #[default]
    Show,
    Get(String),
    Schema,
    Set(String, String),
    Push(String, String),
    Remove(String, String),
    Unset(String),
    Init,
    Edit,
}

pub struct Options {
    op: Operation,
    alias: Option<Alias>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<Operation> = None;
        let mut alias = None;

        #[allow(clippy::never_loop)]
        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("alias") => {
                    let value = parser.value()?;
                    let input = value.to_string_lossy();
                    let input = Alias::from_str(&input)?;

                    alias = Some(input);
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "show" => op = Some(Operation::Show),
                    "schema" => op = Some(Operation::Schema),
                    "edit" => op = Some(Operation::Edit),
                    "init" => op = Some(Operation::Init),
                    "get" => {
                        let key = parser.value()?;
                        let key = key.to_string_lossy();
                        op = Some(Operation::Get(key.to_string()));
                    }
                    "set" => {
                        let key = parser.value()?;
                        let key = key.to_string_lossy();
                        let value = parser.value()?;
                        let value = value.to_string_lossy();

                        op = Some(Operation::Set(key.to_string(), value.to_string()));
                    }
                    "push" => {
                        let key = parser.value()?;
                        let key = key.to_string_lossy();
                        let value = parser.value()?;
                        let value = value.to_string_lossy();

                        op = Some(Operation::Push(key.to_string(), value.to_string()));
                    }
                    "remove" => {
                        let key = parser.value()?;
                        let key = key.to_string_lossy();
                        let value = parser.value()?;
                        let value = value.to_string_lossy();

                        op = Some(Operation::Remove(key.to_string(), value.to_string()));
                    }
                    "unset" => {
                        let key = parser.value()?;
                        let key = key.to_string_lossy();
                        op = Some(Operation::Unset(key.to_string()));
                    }
                    unknown => anyhow::bail!("unknown operation '{unknown}'"),
                },
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                op: op.unwrap_or_default(),
                alias,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let home = ctx.home()?;
    let path = home.config();

    match options.op {
        Operation::Show => {
            let profile = ctx.profile()?;
            term::json::to_pretty(&profile.config, path.as_path())?.print();
        }
        Operation::Schema => {
            term::json::to_pretty(&schemars::schema_for!(Config), path.as_path())?.print();
        }
        Operation::Get(key) => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let key: ConfigPath = key.into();
            let value = temp_config
                .get_mut(&key)
                .ok_or_else(|| ConfigError::Custom(format!("{key} does not exist")))?;
            print_value(value)?;
        }
        Operation::Set(key, value) => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let value = temp_config.set(&key.into(), value.into())?;
            temp_config.write(&path)?;
            print_value(&value)?;
        }
        Operation::Push(key, value) => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let value = temp_config.push(&key.into(), value.into())?;
            temp_config.write(&path)?;
            print_value(&value)?;
        }
        Operation::Remove(key, value) => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let value = temp_config.remove(&key.into(), value.into())?;
            temp_config.write(&path)?;
            print_value(&value)?;
        }
        Operation::Unset(key) => {
            let mut temp_config = RawConfig::from_file(&path)?;
            let value = temp_config.unset(&key.into())?;
            temp_config.write(&path)?;
            print_value(&value)?;
        }
        Operation::Init => {
            if path.try_exists()? {
                anyhow::bail!("configuration file already exists at `{}`", path.display());
            }
            Config::init(
                options.alias.ok_or(anyhow!(
                    "an alias must be provided to initialize a new configuration"
                ))?,
                &path,
            )?;
            term::success!(
                "Initialized new Radicle configuration at {}",
                path.display()
            );
        }
        Operation::Edit => match term::editor::Editor::new(&path)?.extension("json").edit()? {
            Some(_) => {
                term::success!("Successfully made changes to the configuration at {path:?}")
            }
            None => term::info!("No changes were made to the configuration at {path:?}"),
        },
    }

    Ok(())
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
