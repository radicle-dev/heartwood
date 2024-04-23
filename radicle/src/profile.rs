//! Radicle node profile.
//!
//!   $RAD_HOME/                                 # Radicle home
//!     storage/                                 # Storage root
//!       zEQNunJUqkNahQ8VvQYuWZZV7EJB/          # Project git repository
//!       ...                                    # More projects...
//!     keys/
//!       radicle                                # Secret key (PKCS 8)
//!       radicle.pub                            # Public key (PKCS 8)
//!     node/
//!       control.sock                           # Node control socket
//!
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Serialize;
use thiserror::Error;

use crate::crypto::ssh::agent::Agent;
use crate::crypto::ssh::{keystore, Keystore, Passphrase};
use crate::crypto::{PublicKey, Signer};
use crate::explorer::Explorer;
use crate::node::policy::config::store::Read;
use crate::node::{notifications, policy, Alias, AliasStore};
use crate::prelude::Did;
use crate::prelude::NodeId;
use crate::storage::git::transport;
use crate::storage::git::Storage;
use crate::storage::{self, ReadRepository};
use crate::{cli, cob, git, node, web};

/// Environment variables used by radicle.
pub mod env {
    pub use std::env::*;

    /// Path to the radicle home folder.
    pub const RAD_HOME: &str = "RAD_HOME";
    /// Path to the radicle node socket file.
    pub const RAD_SOCKET: &str = "RAD_SOCKET";
    /// Passphrase for the encrypted radicle secret key.
    pub const RAD_PASSPHRASE: &str = "RAD_PASSPHRASE";
    /// RNG seed. Must be convertible to a `u64`.
    pub const RAD_RNG_SEED: &str = "RAD_RNG_SEED";
    /// Private key seed. Used for generating deterministic keypairs.
    pub const RAD_SEED: &str = "RAD_SEED";
    /// Show radicle hints.
    pub const RAD_HINT: &str = "RAD_HINT";
    /// Environment variable to set to overwrite the commit date for both
    /// the author and the committer.
    ///
    /// The format must be a unix timestamp.
    pub const RAD_COMMIT_TIME: &str = "RAD_COMMIT_TIME";
    /// Override the device's local time.
    /// The format must be a unix timestamp.
    pub const RAD_LOCAL_TIME: &str = "RAD_LOCAL_TIME";
    // Turn debug mode on.
    pub const RAD_DEBUG: &str = "RAD_DEBUG";
    // Used to set the Git committer timestamp. Can be overridden
    // to generate deterministic COB IDs.
    pub const GIT_COMMITTER_DATE: &str = "GIT_COMMITTER_DATE";

    /// Commit timestamp to use. Can be overriden by [`RAD_COMMIT_TIME`].
    pub fn commit_time() -> localtime::LocalTime {
        time(RAD_COMMIT_TIME).unwrap_or_else(local_time)
    }

    /// Local time. Can be overriden by [`RAD_LOCAL_TIME`].
    pub fn local_time() -> localtime::LocalTime {
        time(RAD_LOCAL_TIME).unwrap_or_else(localtime::LocalTime::now)
    }

    /// Whether debug mode is on.
    pub fn debug() -> bool {
        var(RAD_DEBUG).is_ok()
    }

    /// Whether or not to show hints.
    pub fn hints() -> bool {
        var(RAD_HINT).is_ok()
    }

    /// Get the configured pager program from the environment.
    pub fn pager() -> Option<String> {
        if let Ok(cfg) = git2::Config::open_default() {
            if let Ok(pager) = cfg.get_string("core.pager") {
                return Some(pager);
            }
        }
        if let Ok(pager) = var("PAGER") {
            return Some(pager);
        }
        None
    }

    /// Get the radicle passphrase from the environment.
    pub fn passphrase() -> Option<super::Passphrase> {
        let Ok(passphrase) = var(RAD_PASSPHRASE) else {
            return None;
        };
        Some(super::Passphrase::from(passphrase))
    }

    /// Get a random number generator from the environment.
    pub fn rng() -> fastrand::Rng {
        if let Ok(seed) = var(RAD_RNG_SEED) {
            let Ok(seed) = seed.parse() else {
                panic!("env::rng: invalid seed specified in `{RAD_RNG_SEED}`");
            };
            fastrand::Rng::with_seed(seed)
        } else {
            fastrand::Rng::new()
        }
    }

    /// Return the seed stored in the [`RAD_SEED`] environment variable, or generate a random one.
    pub fn seed() -> crypto::Seed {
        if let Ok(seed) = var(RAD_SEED) {
            let Ok(seed) = (0..seed.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&seed[i..i + 2], 16))
                .collect::<Result<Vec<u8>, _>>()
            else {
                panic!("env::seed: invalid hexadecimal value set in `{RAD_SEED}`");
            };
            let Ok(seed): Result<[u8; 32], _> = seed.try_into() else {
                panic!("env::seed: invalid seed length set in `{RAD_SEED}`");
            };
            crypto::Seed::new(seed)
        } else {
            crypto::Seed::generate()
        }
    }

    fn time(key: &str) -> Option<localtime::LocalTime> {
        if let Ok(s) = var(key) {
            match s.trim().parse::<u64>() {
                Ok(t) => return Some(localtime::LocalTime::from_secs(t)),
                Err(e) => {
                    panic!("env::time: invalid value {s:?} for `{key}` environment variable: {e}");
                }
            }
        }
        None
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Keystore(#[from] keystore::Error),
    #[error(transparent)]
    MemorySigner(#[from] keystore::MemorySignerError),
    #[error("no radicle profile found at path '{0}'")]
    NotFound(PathBuf),
    #[error("error connecting to ssh-agent: {0}")]
    Agent(#[from] crate::crypto::ssh::agent::Error),
    #[error("radicle key `{0}` is not registered; run `rad auth` to register it with ssh-agent")]
    KeyNotRegistered(PublicKey),
    #[error(transparent)]
    PolicyStore(#[from] node::policy::store::Error),
    #[error(transparent)]
    NotificationsStore(#[from] node::notifications::store::Error),
    #[error(transparent)]
    DatabaseStore(#[from] node::db::Error),
    #[error(transparent)]
    Repository(#[from] storage::RepositoryError),
    #[error(transparent)]
    CobsCache(#[from] cob::cache::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to load configuration from {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("failed to load configuration from {0}: {1}")]
    Load(PathBuf, serde_json::Error),
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
        match fs::File::open(path) {
            Ok(cfg) => {
                serde_json::from_reader(cfg).map_err(|e| ConfigError::Load(path.to_path_buf(), e))
            }
            Err(e) => Err(ConfigError::Io(path.to_path_buf(), e)),
        }
    }

    /// Write configuration to disk.
    pub fn write(&self, path: &Path) -> Result<(), io::Error> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut serializer = serde_json::Serializer::with_formatter(&file, formatter);

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
pub struct Profile {
    pub home: Home,
    pub storage: Storage,
    pub keystore: Keystore,
    pub public_key: PublicKey,
    pub config: Config,
}

impl Profile {
    pub fn init(
        home: Home,
        alias: Alias,
        passphrase: Option<Passphrase>,
        seed: crypto::Seed,
    ) -> Result<Self, Error> {
        let keystore = Keystore::new(&home.keys());
        let public_key = keystore.init("radicle", passphrase, seed)?;
        let config = Config::init(alias.clone(), home.config().as_path())?;
        let storage = Storage::open(
            home.storage(),
            git::UserInfo {
                alias,
                key: public_key,
            },
        )?;
        // Create DBs.
        home.policies_mut()?;
        home.database_mut()?;
        home.notifications_mut()?;

        transport::local::register(storage.clone());

        Ok(Profile {
            home,
            storage,
            keystore,
            public_key,
            config,
        })
    }

    pub fn load() -> Result<Self, Error> {
        let home = self::home()?;
        let keystore = Keystore::new(&home.keys());
        let public_key = keystore
            .public_key()?
            .ok_or_else(|| Error::NotFound(home.path().to_path_buf()))?;
        let config = Config::load(home.config().as_path())?;
        let storage = Storage::open(
            home.storage(),
            git::UserInfo {
                alias: config.alias().clone(),
                key: public_key,
            },
        )?;

        transport::local::register(storage.clone());

        Ok(Profile {
            home,
            storage,
            keystore,
            public_key,
            config,
        })
    }

    pub fn id(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn info(&self) -> git::UserInfo {
        git::UserInfo {
            alias: self.config.alias().clone(),
            key: *self.id(),
        }
    }

    pub fn hints(&self) -> bool {
        if env::hints() {
            return true;
        }
        self.config.cli.hints
    }

    pub fn did(&self) -> Did {
        Did::from(self.public_key)
    }

    pub fn signer(&self) -> Result<Box<dyn Signer>, Error> {
        if !self.keystore.is_encrypted()? {
            let signer = keystore::MemorySigner::load(&self.keystore, None)?;
            return Ok(signer.boxed());
        }

        if let Some(passphrase) = env::passphrase() {
            let signer = keystore::MemorySigner::load(&self.keystore, Some(passphrase))?;
            return Ok(signer.boxed());
        }

        match Agent::connect() {
            Ok(agent) => {
                let signer = agent.signer(self.public_key);
                if signer.is_ready()? {
                    Ok(signer.boxed())
                } else {
                    Err(Error::KeyNotRegistered(self.public_key))
                }
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Get radicle home.
    pub fn home(&self) -> &Home {
        &self.home
    }

    /// Return a read-only handle to the policies of the node.
    pub fn policies(&self) -> Result<policy::config::Config<Read>, policy::store::Error> {
        let path = self.node().join(node::POLICIES_DB_FILE);
        let config = policy::config::Config::new(
            self.config.node.policy,
            self.config.node.scope,
            policy::store::Store::reader(path)?,
        );
        Ok(config)
    }

    /// Return a multi-source store for aliases.
    pub fn aliases(&self) -> Aliases {
        let policies = self.home.policies().ok();
        let db = self.home.database().ok();

        Aliases { policies, db }
    }
}

impl std::ops::Deref for Profile {
    type Target = Home;

    fn deref(&self) -> &Self::Target {
        &self.home
    }
}

impl std::ops::DerefMut for Profile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.home
    }
}

impl AliasStore for Profile {
    fn alias(&self, nid: &NodeId) -> Option<Alias> {
        self.aliases().alias(nid)
    }
}

/// Holds multiple alias stores, and will try
/// them one by one when asking for an alias.
pub struct Aliases {
    policies: Option<policy::store::StoreReader>,
    db: Option<node::Database>,
}

impl AliasStore for Aliases {
    /// Retrieve `alias` of given node.
    /// First looks in `policies.db` and then `addresses.db`.
    fn alias(&self, nid: &NodeId) -> Option<Alias> {
        self.policies
            .as_ref()
            .and_then(|db| db.alias(nid))
            .or_else(|| self.db.as_ref().and_then(|db| db.alias(nid)))
    }
}

/// Get the path to the radicle home folder.
pub fn home() -> Result<Home, io::Error> {
    if let Some(home) = env::var_os(env::RAD_HOME) {
        Ok(Home::new(PathBuf::from(home))?)
    } else if let Some(home) = env::var_os("HOME") {
        Ok(Home::new(PathBuf::from(home).join(".radicle"))?)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Neither `RAD_HOME` nor `HOME` are set",
        ))
    }
}

/// Radicle home.
#[derive(Debug, Clone)]
pub struct Home {
    path: PathBuf,
}

impl TryFrom<PathBuf> for Home {
    type Error = io::Error;

    fn try_from(home: PathBuf) -> Result<Self, Self::Error> {
        Self::new(home)
    }
}

impl Home {
    /// Creates the Radicle Home directories.
    ///
    /// The `home` path is used as the base directory for all
    /// necessary subdirectories.
    ///
    /// If `home` does not already exist then it and any
    /// subdirectories are created using [`fs::create_dir_all`].
    ///
    /// The `home` path is also canonicalized using [`fs::canonicalize`].
    ///
    /// All necessary subdirectories are also created.
    pub fn new(home: impl Into<PathBuf>) -> Result<Self, io::Error> {
        let path = home.into();
        if !path.exists() {
            fs::create_dir_all(path.clone())?;
        }
        let home = Self {
            path: path.canonicalize()?,
        };

        for dir in &[home.storage(), home.keys(), home.node(), home.cobs()] {
            if !dir.exists() {
                fs::create_dir_all(dir)?;
            }
        }

        Ok(home)
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn storage(&self) -> PathBuf {
        self.path.join("storage")
    }

    pub fn config(&self) -> PathBuf {
        self.path.join("config.json")
    }

    pub fn keys(&self) -> PathBuf {
        self.path.join("keys")
    }

    pub fn node(&self) -> PathBuf {
        self.path.join("node")
    }

    pub fn cobs(&self) -> PathBuf {
        self.path.join("cobs")
    }

    pub fn socket(&self) -> PathBuf {
        env::var_os(env::RAD_SOCKET)
            .map(PathBuf::from)
            .unwrap_or_else(|| self.node().join(node::DEFAULT_SOCKET_NAME))
    }

    /// Return a read-write handle to the notifications database.
    pub fn notifications_mut(
        &self,
    ) -> Result<notifications::StoreWriter, notifications::store::Error> {
        let path = self.node().join(node::NOTIFICATIONS_DB_FILE);
        let db = notifications::Store::open(path)?;

        Ok(db)
    }

    /// Return a read-write handle to the policies store of the node.
    pub fn policies_mut(&self) -> Result<policy::store::StoreWriter, policy::store::Error> {
        let path = self.node().join(node::POLICIES_DB_FILE);
        let config = policy::store::Store::open(path)?;

        Ok(config)
    }

    /// Return a handle to a read-only database of the node.
    pub fn database(&self) -> Result<node::Database, node::db::Error> {
        let path = self.node().join(node::NODE_DB_FILE);
        let db = node::Database::reader(path)?;

        Ok(db)
    }

    /// Return a handle to the database of the node.
    pub fn database_mut(&self) -> Result<node::Database, node::db::Error> {
        let path = self.node().join(node::NODE_DB_FILE);
        let db = node::Database::open(path)?;

        Ok(db)
    }

    /// Return a read-only handle for the issues cache.
    pub fn issues<'a, R>(
        &self,
        repository: &'a R,
    ) -> Result<cob::issue::Cache<cob::issue::Issues<'a, R>, cob::cache::StoreReader>, Error>
    where
        R: ReadRepository + cob::Store,
    {
        let path = self.cobs().join(cob::cache::COBS_DB_FILE);
        let db = cob::cache::Store::reader(path)?;
        let store = cob::issue::Issues::open(repository)?;
        Ok(cob::issue::Cache::reader(store, db))
    }

    /// Return a read-write handle for the issues cache.
    pub fn issues_mut<'a, R>(
        &self,
        repository: &'a R,
    ) -> Result<cob::issue::Cache<cob::issue::Issues<'a, R>, cob::cache::StoreWriter>, Error>
    where
        R: ReadRepository + cob::Store,
    {
        let path = self.cobs().join(cob::cache::COBS_DB_FILE);
        let db = cob::cache::Store::open(path)?;
        let store = cob::issue::Issues::open(repository)?;
        Ok(cob::issue::Cache::open(store, db))
    }

    /// Return a read-only handle for the patches cache.
    pub fn patches<'a, R>(
        &self,
        repository: &'a R,
    ) -> Result<cob::patch::Cache<cob::patch::Patches<'a, R>, cob::cache::StoreReader>, Error>
    where
        R: ReadRepository + cob::Store,
    {
        let path = self.cobs().join(cob::cache::COBS_DB_FILE);
        let db = cob::cache::Store::reader(path)?;
        let store = cob::patch::Patches::open(repository)?;
        Ok(cob::patch::Cache::reader(store, db))
    }

    /// Return a read-write handle for the patches cache.
    pub fn patches_mut<'a, R>(
        &self,
        repository: &'a R,
    ) -> Result<cob::patch::Cache<cob::patch::Patches<'a, R>, cob::cache::StoreWriter>, Error>
    where
        R: ReadRepository + cob::Store,
    {
        let path = self.cobs().join(cob::cache::COBS_DB_FILE);
        let db = cob::cache::Store::open(path)?;
        let store = cob::patch::Patches::open(repository)?;
        Ok(cob::patch::Cache::open(store, db))
    }
}

// Private methods.
impl Home {
    /// Return a read-only handle to the policies store of the node.
    fn policies(&self) -> Result<policy::store::StoreReader, policy::store::Error> {
        let path = self.node().join(node::POLICIES_DB_FILE);
        let config = policy::store::Store::reader(path)?;

        Ok(config)
    }
}

#[cfg(test)]
#[cfg(not(target_os = "macos"))]
mod test {
    use std::fs;

    use super::Home;

    // Checks that if we have:
    // '/run/user/1000/.tmpqfK6ih/../.tmpqfK6ih/Radicle/Home'
    //
    // that it gets normalized to:
    // '/run/user/1000/.tmpqfK6ih/Radicle/Home'
    #[test]
    fn canonicalize_home() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Home").join("Radicle");
        fs::create_dir_all(path.clone()).unwrap();

        let last = tmp.path().components().last().unwrap();
        let home = Home::new(
            tmp.path()
                .join("..")
                .join(last)
                .join("Home")
                .join("Radicle"),
        )
        .unwrap();

        assert_eq!(home.path, path);
    }
}
