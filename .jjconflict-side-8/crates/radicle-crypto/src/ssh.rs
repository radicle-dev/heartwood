extern crate alloc;

use alloc::string::{String, ToString as _};

use thiserror::Error;

#[cfg(feature = "std")]
pub mod agent;

#[cfg(feature = "std")]
pub mod keystore;

#[cfg(feature = "std")]
pub use keystore::{Keystore, Passphrase};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExtendedSignaturePemError {
    #[error(transparent)]
    Ssh(#[from] ssh_key::Error),
    #[error(transparent)]
    Signature(#[from] crate::signature::Error),
    #[error("unsupported signature algorithm: {algorithm}")]
    UnsupportedAlgorithm { algorithm: ssh_key::Algorithm },
}

impl crate::ExtendedSignature {
    /// Convert to OpenSSH standard PEM format.
    pub fn to_pem(&self) -> Result<String, ExtendedSignaturePemError> {
        ssh_key::SshSig::new(
            ssh_key::public::KeyData::from(ssh_key::public::Ed25519PublicKey::from(self.key)),
            String::from("radicle"),
            ssh_key::HashAlg::Sha256,
            ssh_key::Signature::new(ssh_key::Algorithm::Ed25519, self.sig.to_vec())?,
        )?
        .to_pem(ssh_key::LineEnding::default())
        .map_err(ExtendedSignaturePemError::from)
    }

    /// Create from OpenSSH PEM format.
    pub fn from_pem(pem: impl AsRef<[u8]>) -> Result<Self, ExtendedSignaturePemError> {
        let sig = ssh_key::SshSig::from_pem(pem)?;

        let key = match sig.public_key() {
            ssh_key::public::KeyData::Ed25519(key) => key.0,
            key_data => {
                return Err(ExtendedSignaturePemError::UnsupportedAlgorithm {
                    algorithm: key_data.algorithm(),
                });
            }
        };

        Ok(Self {
            key: crate::PublicKey(key),
            sig: crate::Signature::try_from(sig.signature().as_bytes())?,
        })
    }
}

pub mod fmt {
    use super::*;
    use crate::PublicKey;

    /// Get the SSH long key from a public key.
    /// This is the output of `ssh-add -L`.
    pub fn key(key: &PublicKey) -> String {
        ssh_key::PublicKey::from(ssh_key::public::Ed25519PublicKey(key.0)).to_string()
    }

    /// Get the SSH key fingerprint from a public key.
    /// This is the output of `ssh-add -l`.
    pub fn fingerprint(key: &PublicKey) -> String {
        ssh_key::PublicKey::from(ssh_key::public::Ed25519PublicKey(key.0))
            .fingerprint(Default::default())
            .to_string()
    }

    #[cfg(test)]
    mod test {
        use super::*;

        use alloc::str::FromStr;

        #[test]
        fn test_key() {
            let pk =
                PublicKey::from_str("z6MktWkM9vcfysWFq1c2aaLjJ6j4PYYg93TLPswR4qtuoAeT").unwrap();

            assert_eq!(
                key(&pk),
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINDoXIrhcnRjnLGUXUFdxhkuy08lkTOwrj2IoGsEX6+Q"
            );
        }

        #[test]
        fn test_fingerprint() {
            let pk =
                PublicKey::from_str("z6MktWkM9vcfysWFq1c2aaLjJ6j4PYYg93TLPswR4qtuoAeT").unwrap();

            assert_eq!(
                fingerprint(&pk),
                "SHA256:gE/Ty4fuXzww49lcnNe9/GI0L7xSEQdFp/v9tOjFwB4"
            );
        }
    }
}
