use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::prelude::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::crypto::{PublicKey, SecretKey};

pub struct UnsafeKeystore {
    path: PathBuf,
}

impl UnsafeKeystore {
    pub fn new<P: AsRef<Path>>(path: &P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn put(&mut self, public: &PublicKey, secret: &SecretKey) -> Result<(), io::Error> {
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

    pub fn get(&self) -> Result<(PublicKey, SecretKey), io::Error> {
        let public = fs::read(self.path.join("radicle.pub"))?;
        let public = String::from_utf8(public).unwrap();
        let public = PublicKey::from_pem(&public).unwrap();

        let secret = fs::read(self.path.join("radicle"))?;
        let secret = String::from_utf8(secret).unwrap();
        let secret = SecretKey::from_pem(&secret).unwrap();

        Ok((public, secret))
    }
}
