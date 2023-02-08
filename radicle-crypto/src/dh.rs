use std::{fmt, ops};

use ed25519_compact::{x25519, Error};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PublicKey(x25519::PublicKey);

impl ops::Deref for PublicKey {
    type Target = x25519::PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = multibase::Base::Base58Btc;
        write!(f, "{}.x25519", multibase::encode(base, *self.0))
    }
}

impl TryFrom<&crate::PublicKey> for PublicKey {
    type Error = Error;

    fn try_from(other: &crate::PublicKey) -> Result<Self, Self::Error> {
        x25519::PublicKey::from_ed25519(other).map(Self)
    }
}

impl From<crate::PublicKey> for PublicKey {
    fn from(other: crate::PublicKey) -> Self {
        Self::try_from(&other).expect("PublicKey::from: public key is expected to be valid")
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::EcPk for PublicKey {
    const COMPRESSED_LEN: usize = 32;
    const CURVE_NAME: &'static str = "Curve25519";

    type Compressed = [u8; 32];

    fn base_point() -> Self {
        unimplemented!()
    }

    fn to_pk_compressed(&self) -> Self::Compressed {
        *self.0
    }

    fn from_pk_compressed(pk: Self::Compressed) -> Result<Self, cyphernet::EcPkInvalid> {
        Ok(Self(x25519::PublicKey::new(pk)))
    }

    fn from_pk_compressed_slice(slice: &[u8]) -> Result<Self, cyphernet::EcPkInvalid> {
        x25519::PublicKey::from_slice(slice)
            .map_err(|_| cyphernet::EcPkInvalid::default())
            .map(Self)
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::display::MultiDisplay<cyphernet::display::Encoding> for PublicKey {
    type Display = String;

    fn display_fmt(&self, _: &cyphernet::display::Encoding) -> Self::Display {
        self.to_string()
    }
}
