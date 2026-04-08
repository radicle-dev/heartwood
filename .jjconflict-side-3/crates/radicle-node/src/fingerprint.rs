//! Fingerprint the public key corresponding to the secret key used by
//! `radicle-node`.
//!
//! This allows users to configure the path to the secret key
//! freely, while ensuring that the key is not changed.
//!
//! In order to achieve this, the fingerprint of the public key
//! derived from the secret key is stored in the Radicle home
//! in a file (usually at `.radicle/node/fingerprint`).
//! When the node starts up and this file does not exist, it is assumed that
//! this is the first time the node is started, and the fingerprint is
//! initialized from the secret key in the keystore.
//! On subsequent startups, the fingerprint of the public key
//! derived from the secret key in the keystore is compared to the
//! fingerprint stored on disk, and if they do not match, the node
//! refuses to start (this last part is implemented in `main.rs`).
//!
//! If the user deletes the fingerprint file, the node will not be able
//! to detect a possible change of the secret key. The consequences of
//! doing this are unclear.

use thiserror::Error;

use radicle::crypto;
use radicle::profile::Home;

/// Fingerprint of a public key.
#[derive(Debug, PartialEq)]
pub struct Fingerprint(String);

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FingerprintVerification {
    Match,
    Mismatch,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("fingerprint file is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

impl Fingerprint {
    /// Return fingerprint of the node, if it exists.
    pub fn read(home: &Home) -> Result<Option<Fingerprint>, Error> {
        match std::fs::read(path(home)) {
            Ok(contents) => Ok(Some(Fingerprint(
                String::from(std::str::from_utf8(contents.as_ref())?)
                    .trim_end()
                    .to_string(),
            ))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::Io(err)),
        }
    }

    /// Initialize the fingerprint of the node with given public key.
    pub fn init(
        home: &Home,
        secret_key: &impl std::ops::Deref<Target = crypto::SecretKey>,
    ) -> Result<(), Error> {
        let public_key = crypto::PublicKey(secret_key.deref().public_key());
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path(home))?;
        {
            use std::io::Write as _;
            file.write_all(crypto::ssh::fmt::fingerprint(&public_key).as_ref())?;
        }

        Ok(())
    }

    /// Verify that the fingerprint of given public key matches self.
    pub fn verify(
        &self,
        secret_key: &impl std::ops::Deref<Target = crypto::SecretKey>,
    ) -> FingerprintVerification {
        let public_key = crypto::PublicKey(secret_key.deref().public_key());
        if crypto::ssh::fmt::fingerprint(&public_key) == self.0 {
            FingerprintVerification::Match
        } else {
            FingerprintVerification::Mismatch
        }
    }
}

/// Return the location of the node fingerprint.
fn path(home: &Home) -> std::path::PathBuf {
    home.node().join("fingerprint")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::ssh::Keystore;

    #[test]
    fn matching() {
        let tmp = tempfile::tempdir().unwrap();
        let home = Home::new(tmp.path()).unwrap();

        let store = Keystore::new(&home.keys());
        store.init("test 1", None, crypto::Seed::default()).unwrap();
        let secret = store.secret_key(None).unwrap().unwrap();

        assert_eq!(Fingerprint::read(&home).unwrap(), None);
        Fingerprint::init(&home, &secret).unwrap();

        let fp = Fingerprint::read(&home).unwrap().unwrap();
        assert_eq!(fp.verify(&secret), FingerprintVerification::Match);

        // Generate a new keypair, which does not match the fingerprint.
        // This simulates the user modifying `~/.radicle/keys`.
        std::fs::remove_dir_all(home.keys()).unwrap();
        store.init("test 1", None, crypto::Seed::default()).unwrap();
        let other_secret = store.secret_key(None).unwrap().unwrap();

        assert_ne!(secret, other_secret);
        // Note that `fp` has not changed since it was initialized from `secret`.
        assert_eq!(fp.verify(&other_secret), FingerprintVerification::Mismatch);
    }
}
