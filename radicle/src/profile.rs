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
use std::path::{Path, PathBuf};
use std::{fs, io};

use thiserror::Error;

use crate::crypto::ssh::agent::Agent;
use crate::crypto::ssh::{keystore, Keystore, Passphrase};
use crate::crypto::{PublicKey, Signer};
use crate::node;
use crate::node::{address, routing, tracking, AliasStore};
use crate::prelude::Did;
use crate::prelude::NodeId;
use crate::storage::git::transport;
use crate::storage::git::Storage;

/// Environment variables used by radicle.
pub mod env {
    pub use std::env::*;

    /// Path to the radicle home folder.
    pub const RAD_HOME: &str = "RAD_HOME";
    /// Path to the radicle node socket file.
    pub const RAD_SOCKET: &str = "RAD_SOCKET";
    /// Passphrase for the encrypted radicle secret key.
    pub const RAD_PASSPHRASE: &str = "RAD_PASSPHRASE";

    pub fn passphrase() -> Option<super::Passphrase> {
        let Ok(passphrase) = std::env::var(RAD_PASSPHRASE) else {
            return None;
        };
        Some(super::Passphrase::from(passphrase))
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Keystore(#[from] keystore::Error),
    #[error(transparent)]
    MemorySigner(#[from] keystore::MemorySignerError),
    #[error("no profile found at the filepath '{0}'")]
    NotFound(PathBuf),
    #[error("error connecting to ssh-agent: {0}")]
    Agent(#[from] crate::crypto::ssh::agent::Error),
    #[error("profile key `{0}` is not registered with ssh-agent")]
    KeyNotRegistered(PublicKey),
    #[error(transparent)]
    TrackingStore(#[from] node::tracking::store::Error),
    #[error(transparent)]
    AddressStore(#[from] node::address::Error),
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub home: Home,
    pub storage: Storage,
    pub keystore: Keystore,
    pub public_key: PublicKey,
}

impl Profile {
    pub fn init(home: Home, passphrase: impl Into<Passphrase>) -> Result<Self, Error> {
        let storage = Storage::open(home.storage())?;
        let keystore = Keystore::new(&home.keys());
        let public_key = keystore.init("radicle", passphrase)?;

        transport::local::register(storage.clone());

        Ok(Profile {
            home,
            storage,
            keystore,
            public_key,
        })
    }

    pub fn load() -> Result<Self, Error> {
        let home = self::home()?;
        let storage = Storage::open(home.storage())?;
        let keystore = Keystore::new(&home.keys());
        let public_key = keystore
            .public_key()?
            .ok_or_else(|| Error::NotFound(home.path().to_path_buf()))?;

        transport::local::register(storage.clone());

        Ok(Profile {
            home,
            storage,
            keystore,
            public_key,
        })
    }

    pub fn id(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn did(&self) -> Did {
        Did::from(self.public_key)
    }

    pub fn signer(&self) -> Result<Box<dyn Signer>, Error> {
        if let Some(passphrase) = env::passphrase() {
            let signer = keystore::MemorySigner::load(&self.keystore, passphrase)?;
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

    /// Return a read-only handle to the tracking configuration of the node.
    pub fn tracking(&self) -> Result<tracking::store::Config, tracking::store::Error> {
        let path = self.home.node().join(node::TRACKING_DB_FILE);
        let config = tracking::store::Config::reader(path)?;

        Ok(config)
    }

    /// Return a read-only handle to the routing database of the node.
    pub fn routing(&self) -> Result<routing::Table, routing::Error> {
        let path = self.home.node().join(node::ROUTING_DB_FILE);
        let router = routing::Table::reader(path)?;

        Ok(router)
    }

    /// Return a handle to the addresses database of the node.
    pub fn addresses(&self) -> Result<address::Book, address::Error> {
        let path = self.home.node().join(node::ADDRESS_DB_FILE);
        let addresses = address::Book::reader(path)?;

        Ok(addresses)
    }

    /// Return a multi-source store for aliases.
    pub fn aliases(&self) -> Result<Aliases, Error> {
        let tracking = self.tracking()?;
        let addresses = self.addresses()?;

        Ok(Aliases {
            tracking,
            addresses,
        })
    }

    /// Return the path to the keys folder.
    pub fn keys(&self) -> PathBuf {
        self.home.keys()
    }

    /// Get the profile home directory.
    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// Get the path to the radicle node socket.
    pub fn socket(&self) -> PathBuf {
        self.home.socket()
    }
}

/// Holds multiple alias stores, and will try
/// them one by one when asking for an alias.
pub struct Aliases {
    tracking: tracking::store::Config,
    addresses: address::Book,
}

impl AliasStore for Aliases {
    /// Retrieve `alias` of given node.
    /// First looks in `tracking.db` and then `addresses.db`.
    fn alias(&self, nid: &NodeId) -> Option<String> {
        self.tracking
            .alias(nid)
            .or_else(|| self.addresses.alias(nid))
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

        for dir in &[home.storage(), home.keys(), home.node()] {
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

    pub fn keys(&self) -> PathBuf {
        self.path.join("keys")
    }

    pub fn node(&self) -> PathBuf {
        self.path.join("node")
    }

    pub fn socket(&self) -> PathBuf {
        env::var_os(env::RAD_SOCKET)
            .map(PathBuf::from)
            .unwrap_or_else(|| self.node().join(node::DEFAULT_SOCKET_NAME))
    }
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::path::Path;

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
        let mut home = Home::new(
            tmp.path()
                .join("..")
                .join(last)
                .join("Home")
                .join("Radicle"),
        )
        .unwrap();
        if cfg!(target_os = "macos") {
            home.path =
                Path::new("/").join(home.path.strip_prefix("/private").unwrap_or(&home.path))
        };

        assert_eq!(home.path, path);
    }
}
