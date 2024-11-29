pub mod key;

use signature::{Signer, Verifier};

// pub struct Did<T = ()> {
//     method: Method,
//     method_id: MethodId,
//     identity: T,
// }

// impl<T: Verifier> Verifier for Did<T> {
//     fn verify(&self, msg: &[u8], signature: &S) -> Result<(), signature::Error> {
//         self.identity.verify(msg, signature)
//     }
// }

// pub struct Method(String);

// pub struct MethodId(String);

// TODO: would we need this? It's general and we could newtype `DidKey`,
// `DidKeri`, etc.
pub struct Did {
    method: Method,
    method_id: MethodId,
}

// TODO: decide if this is actually a good abstraction
pub trait Resolver {
    type Did;
    type Signature;

    fn verifier(self, did: &Self::Did) -> Result<impl Verifier<Self::Signature>, ResolverError>;

    fn signer(self, did: &Self::Did) -> Result<impl Signer<Self::Signature>, ResolverError>;
}

pub struct ResolverError(Box<dyn std::error::Error + Send + Sync + 'static>);

impl ResolverError {
    pub fn new<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self(Box::new(err))
    }
}
pub struct Method(String);

pub struct MethodId(String);
