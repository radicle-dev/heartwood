use std::path::PathBuf;
use std::{env, io};

use crate::crypto::{KeyPair, PublicKey, SecretKey, Signature, Signer};
use crate::keystore::UnsafeKeystore;
use crate::storage::git::Storage;

pub struct UnsafeSigner {
    public: PublicKey,
    secret: SecretKey,
}

impl Signer for UnsafeSigner {
    fn public_key(&self) -> &PublicKey {
        &self.public
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        Signature(self.secret.sign(msg, None))
    }
}

pub struct Profile {
    pub home: PathBuf,
    pub signer: UnsafeSigner,
    pub storage: Storage,
}

impl Profile {
    pub fn init(keypair: KeyPair) -> Result<Self, io::Error> {
        let home = self::home()?;
        let mut keystore = UnsafeKeystore::new(&home.join("keys"));
        let public = keypair.pk.into();
        let signer = UnsafeSigner {
            public,
            secret: keypair.sk,
        };
        let storage = Storage::open(&home.join("storage"))?;

        keystore.put(&signer.public, &signer.secret)?;

        Ok(Profile {
            home,
            signer,
            storage,
        })
    }

    pub fn load() -> Result<Self, io::Error> {
        let home = self::home()?;
        let (public, secret) = UnsafeKeystore::new(&home.join("keys")).get()?;
        let signer = UnsafeSigner { public, secret };
        let storage = Storage::open(&home.join("storage"))?;

        Ok(Profile {
            home,
            signer,
            storage,
        })
    }

    pub fn id(&self) -> &PublicKey {
        self.signer.public_key()
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
