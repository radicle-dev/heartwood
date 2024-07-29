use std::io::Write;
use std::path::Path;
use std::{fmt, fs, io};

use serde::Serialize as _;
use serde_json as json;
use thiserror::Error;

use crate::explorer::Explorer;
use crate::node::config::DefaultSeedingPolicy;
use crate::node::policy::{Policy, Scope};
use crate::node::Alias;
use crate::{cli, node, web};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("configuration JSON error: {0}")]
    Json(#[from] json::Error),
    #[error("configuration error: {0}")]
    Custom(String),
}

/// Local radicle configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Public explorer. This is used for generating links.
    #[serde(default)]
    pub public_explorer: Explorer,
    /// Preferred seeds. These seeds will be used for explorer links
    /// and in other situations when a seed needs to be chosen.
    #[serde(default)]
    pub preferred_seeds: Vec<node::config::ConnectAddress>,
    /// Web configuration.
    #[serde(default)]
    pub web: web::Config,
    /// CLI configuration.
    #[serde(default)]
    pub cli: cli::Config,
    /// Node configuration.
    pub node: node::Config,
}

impl Config {
    /// Create a new, default configuration.
    pub fn new(alias: Alias) -> Self {
        let node = node::Config::new(alias);

        Self {
            public_explorer: Explorer::default(),
            preferred_seeds: node.network.public_seeds(),
            web: web::Config::default(),
            cli: cli::Config::default(),
            node,
        }
    }

    /// Initialize a new configuration. Fails if the path already exists.
    pub fn init(alias: Alias, path: &Path) -> io::Result<Self> {
        let cfg = Config::new(alias);
        cfg.write(path)?;
        Ok(cfg)
    }

    /// Load a configuration from the given path.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let mut cfg: Self = json::from_reader(fs::File::open(path)?)?;

        // Handle deprecated policy configuration.
        // Nb. This will override "seedingPolicy" if set! This code should be removed after 1.0.
        if let (Some(p), Some(s)) = (cfg.node.extra.get("policy"), cfg.node.extra.get("scope")) {
            if let (Ok(policy), Ok(scope)) = (
                json::from_value::<Policy>(p.clone()),
                json::from_value::<Scope>(s.clone()),
            ) {
                log::warn!(target: "radicle", "Overwriting `seedingPolicy` configuration");
                cfg.node.seeding_policy = match policy {
                    Policy::Allow => DefaultSeedingPolicy::Allow { scope },
                    Policy::Block => DefaultSeedingPolicy::Block,
                }
            }
        }
        Ok(cfg)
    }

    /// Write configuration to disk.
    pub fn write(&self, path: &Path) -> Result<(), io::Error> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let formatter = json::ser::PrettyFormatter::with_indent(b"  ");
        let mut serializer = json::Serializer::with_formatter(&file, formatter);

        self.serialize(&mut serializer)?;
        file.write_all(b"\n")?;
        file.sync_all()?;

        Ok(())
    }

    /// Get the user alias.
    pub fn alias(&self) -> &Alias {
        &self.node.alias
    }
}

#[derive(Debug, Clone)]
pub struct TempConfig(json::Value);

#[derive(Debug, Error)]
pub enum ModifyError {
    #[error("the path provided was empty")]
    EmptyPath,
    #[error("could not find an element at the path '{path}'")]
    NotFound { path: ConfigPath },
    #[error("the element at the path '{path}' is not a JSON object")]
    NotObject { path: ConfigPath },
    #[error("the element at the path '{path}' is not a JSON array")]
    NotArray { path: ConfigPath },
    #[error("the parent element of '{key}' is not a JSON object")]
    Upsert { key: String },
}

/// Offers utility functions for editing the configuration. Validates on write.
impl TempConfig {
    /// Creates a temporary configuration, by reading a configuration file from disk.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let file = fs::File::open(path)?;
        let config = json::from_reader(file)?;
        Ok(TempConfig(config))
    }

    /// Get a mutable reference to a configuration value by path, if it exists.
    pub fn get_mut(&mut self, config_path: &ConfigPath) -> Option<&mut json::Value> {
        config_path
            .iter()
            .try_fold(&mut self.0, |current, part| current.get_mut(part))
    }

    /// Delete the value at the the specified path.
    pub fn delete(&mut self, config_path: &ConfigPath) -> Result<json::Value, ModifyError> {
        let last = config_path.last().ok_or(ModifyError::EmptyPath)?;
        let parent = match config_path.parent() {
            Some(parent_path) => {
                self.get_mut(&parent_path)
                    .ok_or_else(|| ModifyError::NotFound {
                        path: config_path.clone(),
                    })?
            }
            None => &mut self.0,
        };

        parent
            .as_object_mut()
            .ok_or_else(|| ModifyError::NotObject {
                path: config_path.clone(),
            })?
            .remove(last);

        Ok(json::Value::Null)
    }

    pub fn add(
        &mut self,
        config_path: &ConfigPath,
        value: ConfigValue,
    ) -> Result<json::Value, ModifyError> {
        if let Some(element) = self.get_mut(config_path) {
            let mut arr = element
                .as_array()
                .ok_or_else(|| ModifyError::NotArray {
                    path: config_path.clone(),
                })?
                .to_vec();
            arr.push(value.into());
            *element = json::Value::Array(arr);
            Ok(element.clone())
        } else {
            self.upsert(config_path, json::Value::Array(vec![value.into()]))
        }
    }

    pub fn remove(
        &mut self,
        config_path: &ConfigPath,
        value: ConfigValue,
    ) -> Result<json::Value, ModifyError> {
        let element = self
            .get_mut(config_path)
            .ok_or_else(|| ModifyError::NotFound {
                path: config_path.clone(),
            })?;
        let arr = element
            .as_array_mut()
            .ok_or_else(|| ModifyError::NotArray {
                path: config_path.clone(),
            })?;
        let value = json::Value::from(value);
        arr.retain(|el| el != &value);
        *element = json::Value::Array(arr.to_owned());
        Ok(element.clone())
    }

    pub fn set(
        &mut self,
        config_path: &ConfigPath,
        value: ConfigValue,
    ) -> Result<json::Value, ModifyError> {
        if let Some(element) = self.get_mut(config_path) {
            *element = value.into();
            Ok(element.clone())
        } else {
            self.upsert(config_path, value)
        }
    }

    /// Writes the configuration, including extra values, to disk. Errors if the config is not
    /// valid.
    pub fn write(&self, path: &Path) -> Result<(), ConfigError> {
        let _valid_config: Config = self.clone().try_into()?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        let formatter = json::ser::PrettyFormatter::with_indent(b"  ");
        let mut serializer = json::Serializer::with_formatter(&file, formatter);

        self.0.serialize(&mut serializer)?;
        file.write_all(b"\n")?;
        file.sync_all()?;

        Ok(())
    }

    /// Create an element at the given path, if it doesn't exist yet.
    fn upsert(
        &mut self,
        config_path: &ConfigPath,
        value: impl Into<json::Value>,
    ) -> Result<json::Value, ModifyError> {
        let mut current = &mut self.0;

        for key in config_path.iter() {
            current = match current {
                json::Value::Object(ref mut map) => {
                    map.entry(key.clone()).or_insert_with(|| json::json!({}))
                }
                _ => return Err(ModifyError::Upsert { key: key.clone() }),
            }
        }

        *current = value.into();
        Ok(current.clone())
    }
}

impl TryInto<Config> for TempConfig {
    type Error = json::Error;

    fn try_into(self) -> Result<Config, Self::Error> {
        json::from_value(self.0)
    }
}

// TODO: this being pub means one can construct an invalid `ConfigValue::Float`
#[derive(Debug, Clone)]
pub enum ConfigValue {
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

impl From<&str> for ConfigValue {
    /// Guess the type of a Value.
    fn from(value: &str) -> Self {
        if let Ok(b) = value.parse::<bool>() {
            ConfigValue::Bool(b)
        } else if let Ok(n) = value.parse::<i64>() {
            ConfigValue::Integer(n)
        } else if let Ok(n) = value.parse::<f64>() {
            if n.is_finite() {
                ConfigValue::Float(n)
            } else {
                ConfigValue::String(n.to_string())
            }
        } else {
            ConfigValue::String(value.to_string())
        }
    }
}

impl From<String> for ConfigValue {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl From<ConfigValue> for json::Value {
    fn from(value: ConfigValue) -> Self {
        match value {
            ConfigValue::Bool(v) => json::Value::Bool(v),
            ConfigValue::Integer(v) => json::Value::Number(v.into()),
            ConfigValue::Float(v) => {
                // Safety: ConfigValue ensures the Float won't be Infinite or NaN
                json::Value::Number(json::Number::from_f64(v).unwrap())
            }
            ConfigValue::String(v) => json::Value::String(v),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ConfigPath(Vec<String>);

impl ConfigPath {
    fn parent(&self) -> Option<Self> {
        self.0.split_last().map(|(_, tail)| Self(tail.to_vec()))
    }

    fn last(&self) -> Option<&String> {
        self.0.last()
    }

    fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }
}

impl fmt::Display for ConfigPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

impl From<String> for ConfigPath {
    fn from(value: String) -> Self {
        let parts: Vec<String> = value.split('.').map(|s| s.to_string()).collect();
        ConfigPath(parts)
    }
}
