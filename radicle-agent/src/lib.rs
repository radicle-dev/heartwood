use radicle_crypto as crypto;

// TODO(finto): we may want to the concept of a "login", which is a verified
// pair of NodeSigner/NodeId + Agent, where the NodeId is associated with the
// Agent – and can be used for refs/namespaces shenanigans

// TODO(finto): this could also extend `Verifier`
// TODO(finto): survey for usage of `sign` and change to `try_sign` instead
pub trait Agent<S = crypto::Signature>: signature::Signer<S> {
    // TODO(lorenz): Add for now so that we stay compatible with COB storage.
    fn public_key(&self) -> &crypto::PublicKey;
}
