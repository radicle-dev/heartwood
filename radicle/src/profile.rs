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
//!       radicle.sock                           # Node control socket
//!
use std::path::{Path, PathBuf};
use std::{fs, io};

use thiserror::Error;

use crate::crypto::ssh::agent::Agent;
use crate::crypto::ssh::{keystore, Keystore, Passphrase};
use crate::crypto::{PublicKey, Signer};
use crate::node;
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

    pub fn read_passphrase() -> Option<super::Passphrase> {
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
        let home = home.init()?;
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

    pub fn signer(&self) -> Result<Box<dyn Signer>, Error> {
        if let Some(passphrase) = env::read_passphrase() {
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

    /// Get `Paths` of profile
    pub fn paths(&self) -> &Home {
        &self.home
    }
}

/// Get the path to the radicle home folder.
pub fn home() -> Result<Home, io::Error> {
    if let Some(home) = env::var_os(env::RAD_HOME) {
        Ok(Home::new(PathBuf::from(home)))
    } else if let Some(home) = env::var_os("HOME") {
        Ok(Home::new(PathBuf::from(home).join(".radicle")))
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

impl From<PathBuf> for Home {
    fn from(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Home {
    pub fn init(self) -> Result<Self, io::Error> {
        fs::create_dir_all(self.node()).ok();

        Ok(self)
    }

    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { path: home.into() }
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
