use radicle::crypto::SecretKey;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Secret(zeroize::Zeroizing<radicle::crypto::SecretKey>);

impl Secret {
    pub fn new(secret_key: zeroize::Zeroizing<radicle::crypto::SecretKey>) -> Self {
        Self(secret_key)
    }

    pub fn into_inner(self) -> zeroize::Zeroizing<radicle::crypto::SecretKey> {
        self.0
    }
}

impl std::ops::Deref for Secret {
    type Target = radicle::crypto::SecretKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<radicle::crypto::SecretKey> for Secret {
    fn from(sk: radicle::crypto::SecretKey) -> Self {
        Self(zeroize::Zeroizing::new(sk))
    }
}

// #[cfg(feature = "cyphernet")]
impl cyphernet::EcSk for Secret {
    type Pk = radicle::crypto::PublicKey;

    fn generate_keypair() -> (Self, Self::Pk)
    where
        Self: Sized,
    {
        let pair = radicle::crypto::KeyPair::generate();
        (SecretKey::from(pair.sk).into(), pair.pk.into())
    }

    fn to_pk(&self) -> Result<Self::Pk, cyphernet::EcSkInvalid> {
        Ok(self.public_key().into())
    }
}

// #[cfg(feature = "cyphernet")]
impl cyphernet::Ecdh for Secret {
    type SharedSecret = [u8; 32];

    fn ecdh(&self, pk: &Self::Pk) -> Result<Self::SharedSecret, cyphernet::EcdhError> {
        self.0.ecdh(pk).map_err(cyphernet::EcdhError::from)
    }
}

#[cfg(any(test, feature = "test"))]
impl Secret {
    pub fn mock_rng(rng: &mut fastrand::Rng) -> Self {
        let mut seed = [0u8; 32];
        rng.fill(&mut seed);
        let seed: radicle::crypto::Seed = radicle::crypto::Seed::from(seed);
        let pair = radicle::crypto::KeyPair::from_seed(seed);
        Self::from(radicle::crypto::SecretKey::from(pair.sk))
    }

    pub fn mock() -> Self {
        let pair = radicle::crypto::KeyPair::generate();
        Self::from(radicle::crypto::SecretKey::from(pair.sk))
    }
}
