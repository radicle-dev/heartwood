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
use std::path::PathBuf;
use std::{env, io};

use crate::crypto::{KeyPair, PublicKey, SecretKey, Signature, Signer};
use crate::keystore::{Error, UnsafeKeystore};
use crate::node;
use crate::storage::git::transport;
use crate::storage::git::Storage;

#[derive(Debug)]
pub struct UnsafeSigner {
    pub public: PublicKey,
    pub secret: SecretKey,
}

impl Signer for UnsafeSigner {
    fn public_key(&self) -> &PublicKey {
        &self.public
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        Signature(self.secret.sign(msg, None))
    }
}

#[derive(Debug)]
pub struct Profile {
    pub home: PathBuf,
    pub signer: UnsafeSigner,
    pub storage: Storage,
}

impl Profile {
    pub fn init(keypair: KeyPair) -> Result<Self, Error> {
        let home = self::home()?;
        let mut keystore = UnsafeKeystore::new(&home.join("keys"));
        let public = keypair.pk.into();
        let signer = UnsafeSigner {
            public,
            secret: keypair.sk,
        };
        let storage = Storage::open(&home.join("storage"))?;

        transport::local::register(storage.clone());
        keystore.put(&signer.public, &signer.secret)?;

        Ok(Profile {
            home,
            signer,
            storage,
        })
    }

    pub fn load() -> Result<Self, Error> {
        let home = self::home()?;
        let (public, secret) = UnsafeKeystore::new(&home.join("keys")).get()?;
        let signer = UnsafeSigner { public, secret };
        let storage = Storage::open(&home.join("storage"))?;

        transport::local::register(storage.clone());

        Ok(Profile {
            home,
            signer,
            storage,
        })
    }

    /// Return a connection to the locally running node.
    pub fn node(&self) -> Result<node::Connection, node::Error> {
        node::Connection::connect(self.socket())
    }

    pub fn id(&self) -> &PublicKey {
        self.signer.public_key()
    }

    /// Get the path to the radicle node socket.
    pub fn socket(&self) -> PathBuf {
        env::var_os("RAD_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join("node").join(node::DEFAULT_SOCKET_NAME))
    }
}

/// Get the path to the radicle home folder.
pub fn home() -> Result<PathBuf, io::Error> {
    if let Some(home) = env::var_os("RAD_HOME") {
        Ok(PathBuf::from(home))
    } else if let Some(home) = env::var_os("HOME") {
        Ok(PathBuf::from(home).join(".radicle"))
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Neither `RAD_HOME` nor `HOME` are set",
        ))
    }
}
