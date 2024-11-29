use crypto::ssh::agent::Agent;
use signature::{Signer, Verifier};
use thiserror::Error;

use crate::crypto;

use super::{Did, Resolver, ResolverError};

pub struct DidKey {
    did: Did,
    public_key: crypto::PublicKey,
}

pub enum Error {
    NotKey,
}

impl DidKey {
    pub fn new(did: Did) -> Result<Self, Error> {
        // if !did.method == "key" {
        //     return Err(Error::NotKey);
        // }
        Ok(todo!())
    }

    pub fn public_key(&self) -> &crypto::PublicKey {
        &self.public_key
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("TODO")]
    KeyNotRegistered(crypto::PublicKey),
    #[error(transparent)]
    IsReady(#[from] crypto::ssh::agent::Error),
}

impl Resolver for Agent {
    type Did = DidKey;
    type Signature = crypto::Signature;

    fn verifier(self, did: &Self::Did) -> Result<impl Verifier<Self::Signature>, ResolverError> {
        Ok(did.public_key)
    }

    fn signer(self, did: &Self::Did) -> Result<impl Signer<Self::Signature>, ResolverError> {
        let signer = Agent::signer(self, did.public_key);
        if signer.is_ready().map_err(ResolverError::new)? {
            Ok(signer)
        } else {
            Err(ResolverError::new(AgentError::KeyNotRegistered(
                did.public_key,
            )))
        }
    }
}
