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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    pub fn init(alias: Alias, path: &Path) -> Result<Self, ConfigError> {
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
    pub fn write(&self, path: &Path) -> Result<(), ConfigError> {
        let value = json::to_value(self)?;
        let tmp = RawConfig(value);
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;

        tmp.write_file(file)
    }

    /// Get the user alias.
    pub fn alias(&self) -> &Alias {
        &self.node.alias
    }
}

/// Offers utility functions for editing the configuration. Validates on write.
#[derive(Debug, Clone)]
pub struct RawConfig(json::Value);

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

impl RawConfig {
    /// Creates a temporary configuration, by reading a configuration file from disk.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let file = fs::File::open(path)?;
        let config = json::from_reader(file)?;
        Ok(RawConfig(config))
    }

    /// Get a mutable reference to a configuration value by path, if it exists.
    pub fn get_mut(&mut self, config_path: &ConfigPath) -> Option<&mut json::Value> {
        config_path
            .iter()
            .try_fold(&mut self.0, |current, part| current.get_mut(part))
    }

    /// Delete the specified path.
    pub fn unset(&mut self, config_path: &ConfigPath) -> Result<json::Value, ModifyError> {
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

    pub fn push(
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
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        self.write_file(file)
    }

    /// Write to an open file.
    fn write_file(&self, mut file: fs::File) -> Result<(), ConfigError> {
        let _valid_config: Config = self.clone().try_into()?;
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
                    map.entry(key).or_insert_with(|| json::json!({}))
                }
                _ => {
                    return Err(ModifyError::Upsert {
                        key: key.to_owned(),
                    })
                }
            }
        }
        *current = value.into();

        Ok(current.clone())
    }
}

impl TryInto<Config> for RawConfig {
    type Error = json::Error;

    fn try_into(self) -> Result<Config, Self::Error> {
        json::from_value(self.0)
    }
}

/// A struct that ensures all values are safe for JSON serialization, including handling special
/// floating point values like `NaN` and `Infinity`. Use the `From<&str>` implementation to create an instance.
pub struct ConfigValue(RawConfigValue);

/// This enum represents raw configuration values and should not be used directly.
/// Use the `ConfigValue` type, which validates values using its `From<&str>` implementation.
#[derive(Debug, Clone)]
enum RawConfigValue {
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

impl From<&str> for ConfigValue {
    /// Guess the type of a Value.
    fn from(value: &str) -> Self {
        if let Ok(b) = value.parse::<bool>() {
            ConfigValue(RawConfigValue::Bool(b))
        } else if let Ok(n) = value.parse::<i64>() {
            ConfigValue(RawConfigValue::Integer(n))
        } else if let Ok(n) = value.parse::<f64>() {
            // NaN and Infinite can't be properly serialized to JSON
            if n.is_finite() {
                ConfigValue(RawConfigValue::Float(n))
            } else {
                ConfigValue(RawConfigValue::String(value.to_string()))
            }
        } else {
            ConfigValue(RawConfigValue::String(value.to_string()))
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
            ConfigValue(RawConfigValue::Bool(v)) => json::Value::Bool(v),
            ConfigValue(RawConfigValue::Integer(v)) => json::Value::Number(v.into()),
            ConfigValue(RawConfigValue::Float(v)) => {
                // SAFETY: ConfigValue ensures the Float won't be Infinite or NaN.
                #[allow(clippy::unwrap_used)]
                json::Value::Number(json::Number::from_f64(v).unwrap())
            }
            ConfigValue(RawConfigValue::String(v)) => json::Value::String(v),
        }
    }
}

/// Configuration attribute path.
#[derive(Default, Debug, Clone)]
pub struct ConfigPath(Vec<String>);

impl ConfigPath {
    fn parent(&self) -> Option<Self> {
        self.0.split_last().map(|(_, tail)| Self(tail.to_vec()))
    }

    fn last(&self) -> Option<&str> {
        self.0.last().map(AsRef::as_ref)
    }

    fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|s| s.as_str())
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    #[test]
    fn schema() {
        use super::Config;
        use crate::prelude::Alias;
        use serde_json::to_value;

        let schema = to_value(schemars::schema_for!(Config)).unwrap();
        let config = to_value(Config::new(Alias::new("schema"))).unwrap();
        jsonschema::validate(&schema, &config)
            .expect("generated configuration should validate under generated JSON Schema");
    }
}
