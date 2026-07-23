use std::path::{Path, PathBuf};
use std::{fs, io};

use serde_json as json;
use thiserror::Error;

use crate::explorer::Explorer;
use crate::node::Alias;
use crate::node::config::DefaultSeedingPolicy;
use crate::node::policy::{Policy, Scope};
use crate::{cli, node, web};

#[derive(Debug, Error)]
#[error("writing configuration to {path:?} failed: {kind}")]
pub struct WriteError {
    path: PathBuf,
    kind: WriteErrorKind,
}

impl WriteError {
    fn to_json<P>(path: P, err: serde_json::Error) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            path: path.as_ref().to_path_buf(),
            kind: WriteErrorKind::ToJson { err },
        }
    }

    fn write_file<P>(path: P, err: io::Error) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            path: path.as_ref().to_path_buf(),
            kind: WriteErrorKind::WriteFile { err },
        }
    }
}

#[derive(Debug, Error)]
pub enum WriteErrorKind {
    #[error("could not open due to {err}")]
    OpenFile {
        #[source]
        err: io::Error,
    },
    #[error("could not convert to JSON due to {err}")]
    ToJson {
        #[source]
        err: json::Error,
    },
    #[error("could not write to file due to {err}")]
    WriteFile {
        #[source]
        err: io::Error,
    },
    #[error("validation failure due to {err}")]
    Validate {
        #[source]
        err: serde_json::Error,
    },
    #[error("could not serialize due to {err}")]
    SerializeJson {
        #[source]
        err: serde_json::Error,
    },
}

#[derive(Debug, Error)]
pub enum InitError {
    #[error("failed to initialize configuration file {path:?}: {err}")]
    Write {
        path: PathBuf,
        #[source]
        err: WriteError,
    },
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error(
        "failed to open configuration file {path:?}: {err}, perhaps you need to initialise one `rad config init --alias <alias>`"
    )]
    File {
        path: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("failed to parse JSON of configuration file {path:?}: {err}")]
    Json {
        path: PathBuf,
        #[source]
        err: serde_json::Error,
    },
}

/// Local Radicle configuration.
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
    #[serde(default, skip_serializing_if = "crate::serde_ext::is_default")]
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
    pub fn init(alias: Alias, path: &Path) -> Result<Self, InitError> {
        let cfg = Config::new(alias);
        cfg.write(path).map_err(|err| InitError::Write {
            path: path.to_path_buf(),
            err,
        })?;
        Ok(cfg)
    }

    /// Load a configuration from the given path.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let file = fs::File::open(path).map_err(|err| LoadError::File {
            path: path.to_path_buf(),
            err,
        })?;
        let mut cfg: Self = json::from_reader(file).map_err(|err| LoadError::Json {
            path: path.to_path_buf(),
            err,
        })?;

        // Handle deprecated policy configuration.
        // Nb. This will override "seedingPolicy" if set! This code should be removed after 1.0.
        if let (Some(p), Some(s)) = (cfg.node.extra.get("policy"), cfg.node.extra.get("scope"))
            && let (Ok(policy), Ok(scope)) = (
                json::from_value::<Policy>(p.clone()),
                json::from_value::<Scope>(s.clone()),
            )
        {
            log::warn!(target: "radicle", "Overwriting `seedingPolicy` configuration");
            cfg.node.seeding_policy = match policy {
                Policy::Allow => DefaultSeedingPolicy::Allow {
                    scope: node::config::Scope::explicit(scope),
                },
                Policy::Block => DefaultSeedingPolicy::Block,
            }
        }
        Ok(cfg)
    }

    /// Write configuration to disk.
    pub fn write(&self, path: &Path) -> Result<(), WriteError> {
        let contents = json::to_vec_pretty(self).map_err(|err| WriteError::to_json(path, err))?;
        fs::write(path, contents).map_err(|err| WriteError::write_file(path, err))
    }

    /// Get the user alias.
    pub fn alias(&self) -> &Alias {
        &self.node.alias
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    #[cfg(feature = "schemars")]
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
