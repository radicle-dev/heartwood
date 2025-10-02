use crypto::{PublicKey, Signature, SigningKey, VerifyingKey, signature, ssh::agent::AgentSigner};

/// Wraps well-known implementations of [`signature::Signer`] and
/// [`signature::Keypair`].
///
/// This is used to abstract over the different ways of signing (references and
/// COB operations), such as using a [`SigningKey`] directly (usually loaded via
/// [`crypto::ssh::Keystore`]) or SSH Agent (via [`AgentSigner`]).
pub enum Signer {
    Key(SigningKey),
    Agent(AgentSigner),
}

impl signature::Signer<Signature> for Signer {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        match self {
            Signer::Key(key) => key.try_sign(msg),
            Signer::Agent(agent) => agent.try_sign(msg),
        }
    }
}

impl signature::Keypair for Signer {
    type VerifyingKey = VerifyingKey;

    fn verifying_key(&self) -> Self::VerifyingKey {
        match self {
            Signer::Key(key) => key.verifying_key(),
            Signer::Agent(agent) => agent.verifying_key(),
        }
    }
}

impl AsRef<PublicKey> for Signer {
    fn as_ref(&self) -> &PublicKey {
        match self {
            Signer::Key(key) => key.as_ref(),
            Signer::Agent(agent) => agent.as_ref(),
        }
    }
}
