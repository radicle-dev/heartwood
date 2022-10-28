use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::prelude::OpenOptionsExt;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::crypto;
use crate::crypto::{PublicKey, SecretKey};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid key file format: {0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("invalid key format: {0}")]
    Crypto(#[from] crypto::Error),
}

pub struct UnsafeKeystore {
    path: PathBuf,
}

impl UnsafeKeystore {
    pub fn new<P: AsRef<Path>>(path: &P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn put(&mut self, public: &PublicKey, secret: &SecretKey) -> Result<(), Error> {
        // TODO: Zeroize secret key.
        let public = public.to_pem();
        let secret = secret.to_pem();

        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.path)?;

        fs::OpenOptions::new()
            .mode(0o644)
            .create_new(true)
            .write(true)
            .open(self.path.join("radicle.pub"))?
            .write_all(public.as_bytes())?;

        fs::OpenOptions::new()
            .mode(0o600)
            .create_new(true)
            .write(true)
            .open(self.path.join("radicle"))?
            .write_all(secret.as_bytes())?;

        Ok(())
    }

    pub fn get(&self) -> Result<Option<(PublicKey, SecretKey)>, Error> {
        let public = self.path.join("radicle.pub");
        let secret = self.path.join("radicle");
        if !public.exists() && !secret.exists() {
            return Ok(None);
        }

        let public = fs::read(public)?;
        let public = String::from_utf8(public)?;
        let public = PublicKey::from_pem(&public)?;

        let secret = fs::read(secret)?;
        let secret = String::from_utf8(secret)?;
        let secret = SecretKey::from_pem(&secret)?;

        Ok(Some((public, secret)))
    }
}
