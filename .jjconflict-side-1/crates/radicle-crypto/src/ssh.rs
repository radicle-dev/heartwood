pub mod agent;
pub mod keystore;

use thiserror::Error;

use crate as crypto;

pub use keystore::{Keystore, Passphrase};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExtendedSignatureError {
    #[error(transparent)]
    Ssh(#[from] ssh_key::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
    #[error("unsupported signature algorithm")]
    UnsupportedAlgorithm,
}

/// Signature with public key, used for SSH signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedSignature {
    pub key: crypto::PublicKey,
    pub sig: crypto::Signature,
}

impl From<ExtendedSignature> for crypto::Signature {
    fn from(ExtendedSignature { sig, .. }: ExtendedSignature) -> Self {
        sig
    }
}

impl ExtendedSignature {
    /// Create a new extended signature.
    pub fn new(public_key: crypto::PublicKey, signature: crypto::Signature) -> Self {
        Self {
            key: public_key,
            sig: signature,
        }
    }

    /// Convert to OpenSSH standard PEM format.
    pub fn to_pem(&self) -> Result<String, ExtendedSignatureError> {
        ssh_key::SshSig::new(
            ssh_key::public::KeyData::from(ssh_key::public::Ed25519PublicKey(**self.key)),
            String::from("radicle"),
            ssh_key::HashAlg::Sha256,
            ssh_key::Signature::new(ssh_key::Algorithm::Ed25519, **self.sig)?,
        )?
        .to_pem(ssh_key::LineEnding::default())
        .map_err(ExtendedSignatureError::from)
    }

    /// Create from OpenSSH PEM format.
    pub fn from_pem(pem: impl AsRef<[u8]>) -> Result<Self, ExtendedSignatureError> {
        let sig = ssh_key::SshSig::from_pem(pem)?;

        Ok(Self {
            key: crypto::PublicKey::from(
                sig.public_key()
                    .ed25519()
                    .ok_or(ExtendedSignatureError::UnsupportedAlgorithm)?
                    .0,
            ),
            sig: crypto::Signature::try_from(sig.signature().as_bytes())?,
        })
    }

    /// Verify the signature for a given payload.
    pub fn verify(&self, payload: &[u8]) -> bool {
        self.key.verify(payload, &self.sig).is_ok()
    }
}

pub mod fmt {
    use crate::PublicKey;

    /// Get the SSH long key from a public key.
    /// This is the output of `ssh-add -L`.
    pub fn key(key: &PublicKey) -> String {
        ssh_key::PublicKey::from(*key).to_string()
    }

    /// Get the SSH key fingerprint from a public key.
    /// This is the output of `ssh-add -l`.
    pub fn fingerprint(key: &PublicKey) -> String {
        ssh_key::PublicKey::from(*key)
            .fingerprint(Default::default())
            .to_string()
    }

    #[cfg(test)]
    mod test {
        use std::str::FromStr;

        use super::*;
        use crate::PublicKey;

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
