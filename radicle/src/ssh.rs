pub mod agent;

use std::io;
use std::mem;
use std::ops::DerefMut;

use byteorder::{BigEndian, WriteBytesExt};
use thiserror::Error;
use zeroize::Zeroizing;

use radicle_ssh::encoding;
use radicle_ssh::encoding::Encoding as _;

use crate::crypto;
use crate::crypto::PublicKey;

pub mod fmt {
    use radicle_ssh::encoding::Encoding as _;

    use crate::crypto::PublicKey;

    /// Get the SSH long key from a public key.
    /// This is the output of `ssh-add -L`.
    pub fn key(key: &PublicKey) -> String {
        let mut buf = Vec::new();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(key.as_ref());

        base64::encode_config(buf, base64::STANDARD_NO_PAD)
    }

    /// Get the SSH key fingerprint from a public key.
    /// This is the output of `ssh-add -l`.
    pub fn fingerprint(key: &PublicKey) -> String {
        use sha2::Digest;

        let mut buf = Vec::new();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(key.as_ref());

        let sha = sha2::Sha256::digest(&buf).to_vec();
        let encoded = base64::encode_config(sha, base64::STANDARD_NO_PAD);

        format!("SHA256:{}", encoded)
    }

    #[cfg(test)]
    mod test {
        use std::str::FromStr;

        use super::*;
        use crate::crypto::PublicKey;

        #[test]
        fn test_key() {
            let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();

            assert_eq!(
                key(&pk),
                "AAAAC3NzaC1lZDI1NTE5AAAAINDoXIrhcnRjnLGUXUFdxhkuy08lkTOwrj2IoGsEX6+Q"
            );
        }

        #[test]
        fn test_fingerprint() {
            let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();

            assert_eq!(
                fingerprint(&pk),
                "SHA256:gE/Ty4fuXzww49lcnNe9/GI0L7xSEQdFp/v9tOjFwB4"
            );
        }
    }
}

#[derive(Debug, Error)]
pub enum PublicKeyError {
    #[error(transparent)]
    Invalid(#[from] crypto::Error),
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
}

impl radicle_ssh::key::Public for PublicKey {
    type Error = PublicKeyError;

    fn write_blob(&self, buf: &mut Zeroizing<Vec<u8>>) -> usize {
        let mut n = 0;
        let typ = b"ssh-ed25519";
        let size = typ.len() + self.len() + mem::size_of::<u32>() * 2;

        buf.write_u32::<BigEndian>(size as u32)
            .expect("writing to a vector never fails");

        n += mem::size_of::<u32>(); // The blob size.
        n += buf.extend_ssh_string(typ);
        n += buf.extend_ssh_string(&self[..]);
        n
    }

    fn read(r: &mut encoding::Cursor) -> Result<Option<Self>, Self::Error> {
        match r.read_string()? {
            b"ssh-ed25519" => {
                let s = r.read_string()?;
                let p = PublicKey::try_from(s)?;

                Ok(Some(p))
            }
            _ => Ok(None),
        }
    }
}

// FIXME: Should zeroize, or we should be creating our own type
// in `crypto`.
struct SecretKey(crypto::SecretKey);

impl From<crypto::SecretKey> for SecretKey {
    fn from(other: crypto::SecretKey) -> Self {
        Self(other)
    }
}

#[derive(Debug, Error)]
pub enum SigningKeyError {
    #[error(transparent)]
    Encoding(#[from] encoding::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl radicle_ssh::key::Private for SecretKey {
    type Error = SigningKeyError;

    fn read(r: &mut encoding::Cursor) -> Result<Option<(Vec<u8>, Self)>, Self::Error> {
        match r.read_string()? {
            b"ssh-ed25519" => {
                let public = r.read_string()?;
                let pair = r.read_string()?;
                let _comment = r.read_string()?;
                let key = crypto::SecretKey::from_slice(pair).unwrap();

                Ok(Some((public.to_vec(), SecretKey(key))))
            }
            _ => Ok(None),
        }
    }

    fn write(&self, buf: &mut Zeroizing<Vec<u8>>) -> Result<(), Self::Error> {
        let public = self.0.public_key();

        buf.extend_ssh_string(b"ssh-ed25519");
        buf.extend_ssh_string(public.as_ref());
        buf.deref_mut().write_u32::<BigEndian>(64)?;
        buf.extend(&*self.0);
        buf.extend_ssh_string(b"radicle");

        Ok(())
    }

    fn write_signature<Bytes: AsRef<[u8]>>(
        &self,
        buf: &mut Zeroizing<Vec<u8>>,
        to_sign: Bytes,
    ) -> Result<(), Self::Error> {
        let name = "ssh-ed25519";
        let signature: [u8; 64] = *self.0.sign(to_sign.as_ref(), None);

        buf.deref_mut()
            .write_u32::<BigEndian>((name.len() + signature.len() + 8) as u32)?;
        buf.extend_ssh_string(name.as_bytes());
        buf.extend_ssh_string(&signature);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use quickcheck_macros::quickcheck;
    use zeroize::Zeroizing;

    use super::SecretKey;
    use crate::crypto;
    use crate::crypto::PublicKey;
    use crate::test::arbitrary::ByteArray;
    use radicle_ssh::agent::client::{AgentClient, ClientStream, Error};
    use radicle_ssh::encoding::Reader;
    use radicle_ssh::key::Private as _;

    #[derive(Clone, Default)]
    struct DummyStream {
        incoming: Arc<Mutex<Zeroizing<Vec<u8>>>>,
    }

    impl ClientStream for DummyStream {
        fn connect_socket<P>(_path: P) -> Result<AgentClient<Self>, Error>
        where
            P: AsRef<std::path::Path> + Send,
        {
            panic!("This function should never be called!")
        }

        fn read_response(&mut self, buf: &mut Zeroizing<Vec<u8>>) -> Result<(), Error> {
            *self.incoming.lock().unwrap() = buf.clone();

            Ok(())
        }
    }

    #[quickcheck]
    fn prop_encode_decode_sk(input: ByteArray<64>) {
        let mut buf = Vec::new().into();
        let sk = crypto::SecretKey::new(input.into_inner());
        SecretKey(sk).write(&mut buf).unwrap();

        let mut cursor = buf.reader(0);
        let (_, output) = SecretKey::read(&mut cursor).unwrap().unwrap();

        assert_eq!(sk, output.0);
    }

    #[test]
    fn test_agent_encoding_remove() {
        use std::str::FromStr;

        let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();
        let expected = [
            0, 0, 0, 56, // Message length
            18, // Message type (remove identity)
            0, 0, 0, 51, // Key blob length
            0, 0, 0, 11, // Key type length
            115, 115, 104, 45, 101, 100, 50, 53, 53, 49, 57, // Key type
            0, 0, 0, 32, // Key length
            208, 232, 92, 138, 225, 114, 116, 99, 156, 177, 148, 93, 65, 93, 198, 25, 46, 203, 79,
            37, 145, 51, 176, 174, 61, 136, 160, 107, 4, 95, 175, 144, // Key
        ];

        let stream = DummyStream::default();
        let mut agent = AgentClient::connect(stream.clone());

        agent.remove_identity(&pk).unwrap();

        assert_eq!(
            stream.incoming.lock().unwrap().as_slice(),
            expected.as_slice()
        );
    }

    #[test]
    fn test_agent_encoding_sign() {
        use std::str::FromStr;

        let pk = PublicKey::from_str("zF4VJZgNEeL1niWmKu1NtT1B4ZyGpjACyhs2VEZvtsws5").unwrap();
        let expected = [
            0, 0, 0, 73, // Message length
            13, // Message type (sign request)
            0, 0, 0, 51, // Key blob length
            0, 0, 0, 11, // Key type length
            115, 115, 104, 45, 101, 100, 50, 53, 53, 49, 57, // Key type
            0, 0, 0, 32, // Public key
            208, 232, 92, 138, 225, 114, 116, 99, 156, 177, 148, 93, 65, 93, 198, 25, 46, 203, 79,
            37, 145, 51, 176, 174, 61, 136, 160, 107, 4, 95, 175, 144, // Key
            0, 0, 0, 9, // Length of data to sign
            1, 2, 3, 4, 5, 6, 7, 8, 9, // Data to sign
            0, 0, 0, 0, // Signature flags
        ];

        let stream = DummyStream::default();
        let mut agent = AgentClient::connect(stream.clone());
        let data: Zeroizing<Vec<u8>> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9].into();

        agent.sign_request(&pk, data).ok();

        assert_eq!(
            stream.incoming.lock().unwrap().as_slice(),
            expected.as_slice()
        );
    }
}
