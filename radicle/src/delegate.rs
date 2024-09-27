use core::fmt;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

use cyphernet::ed25519;
use radicle::crypto::PublicKey;

use nonempty::NonEmpty;
use serde_json as json;
use thiserror::Error;

use crate::node::{Alias, NodeId};

pub struct Assignee {
    /// The alias this assignee is using.
    alias: Option<Alias>,
    /// The DID method that the assignee is associated with.
    did: Did,
}

pub struct Author {
    /// The alias this author is using.
    alias: Option<Alias>,
    /// The DID method that the author is associated with.
    did: Did,
    /// The key the author signed with.
    key: PublicKey,
}

pub trait Verifier {
    type Error;

    /// FIXME: Do we need this function?
    fn did(&self) -> &Did;

    fn verify(&self, msg: &[u8], signature: &crypto::Signature) -> Result<bool, Self::Error>;
}

struct DidKeyVerifier {
    did: Did,
    public_key: ed25519::PublicKey,
}

impl DidKeyVerifier {
    pub fn new(did: Did) -> Self {
        if did.method != "key" {
            panic!("Invalid DID method: {}", self.did.method);
        }
        Self { did, public_key: todo!("deserialize did") }
    }
}

impl Verifier for DidKeyVerifier {
    type Error;

    fn did(&self) -> &Did {
        &self.did
    }

    fn verify(&self, msg: &[u8], signature: &crypto::Signature) -> Result<bool, Self::Error> {
        self.public_key.verify(msg, signature)
    }
}

struct DidKeriVerifier {
    did: Did,
}

impl Verifier for DidKeriVerifier {
    type Error;

    fn did(&self) -> &Did {
        todo!()
    }

    fn verify(&self, msg: &[u8], signature: &crypto::Signature) -> Result<bool, Self::Error> {
        todo!("do a complicated verification procedure as specced by KERI")
    }
}

pub struct GetAuthor {
    did: Did,
}

pub struct CreateAuthor {
    alias: Option<Alias>,
    did: Did,
}

pub struct Identifier(String);

pub trait DelegateIdentifier {
    fn as_identifier(&self) -> Identifier;
}

#[derive(Debug, Error)]
pub enum CreateError {
    #[error("{0} already exists")]
    Duplicate(Did),
    #[error("could not verify ownership of {0}")]
    Verification(Did),
    #[error("failed to create author: {0}")]
    Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

pub trait DelegateRepository {
    type CreateError;
    type ResolveError;
    type Request;

    fn create(&self, request: Self::Request) -> Result<Document, Self::CreateError>;
    fn resolve(&self, id: String) -> Result<Option<Document>, Self::ResolveError>;
}

// did                = "did:" method-name ":" method-specific-id
// method-name        = 1*method-char
// method-char        = %x61-7A / DIGIT
// method-specific-id = *( *idchar ":" ) 1*idchar
// idchar             = ALPHA / DIGIT / "." / "-" / "_" / pct-encoded
// pct-encoded        = "%" HEXDIG HEXDIG
/// DID Syntax
#[derive(Debug)]
pub struct Did {
    method: String,
    identifier: String,
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "did:{}:{}", self.method, self.identifier)
    }
}

pub struct DidUrl {
    did: Did,
    path: Option<String>,
    query: Option<String>,
    fragment: Option<String>,
}

pub struct Document {
    id: Did,
    also_known_as: Option<AlsoKnownAs>,
    controller: Option<Controller>,
    verification_method: Option<Vec<VerificationMethod>>,
    authentication: Option<Authentication>,
    assertion_ethod: Option<Assertion>,
    capability_invocation: Option<CapabilityInvocation>,
    capability_delegation: Option<CapabilityDelegatation>,
    service: Option<Service>,
}

/// URIs
pub struct AlsoKnownAs(Vec<String>);

pub enum Controller {
    Single(Did),
    Multi(NonEmpty<Did>),
}

pub enum Authentication {
    Controller(Did),
    Method(VerificationMethod),
}

pub enum Assertion {
    Controller(Did),
    Method(VerificationMethod),
}

pub enum KeyAgreement {
    Controller(Did),
    Method(VerificationMethod),
}

pub enum CapabilityInvocation {
    Controller(Did),
    Method(VerificationMethod),
}

pub enum CapabilityDelegatation {
    Controller(Did),
    Method(VerificationMethod),
}

pub struct VerificationMethod {
    id: DidUrl,
    controller: Did,
    pk: PublicKeyMethod,
}

pub enum PublicKeyMethod {
    Jwk,
    Multibase(String),
}

pub struct Service {
    /// URI
    id: String,
    service_type: Vec<String>,
    service_endpoint: Endpoint,
}

pub enum Endpoint {
    Uri(String),
    Map(json::Map<String, json::Value>),
    Set(Vec<json::Value>),
}

pub trait DelegateResolver {
    type Identifier;
    type Error;

    fn resolve(&self, id: Self::Identifier) -> Result<Option, Self::Error>;
}

/// A delegate needs to know how to resolve to which `NodeId` it is acting for
/// in a given context.
///
/// The `NodeId` is essentially the Ed25519 public key.
pub trait DelegateNode {
    type Index;

    fn node_id(&self, index: Self::Index) -> NodeId;
}

#[derive(Clone, Debug)]
pub struct Votes<D, T>(BTreeMap<D, T>);

impl<D, T> Default for Votes<D, T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[derive(Clone, Debug)]
pub struct Tally<T>(HashMap<T, usize>);

impl<T> Default for Tally<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> Tally<T>
where
    T: Eq + Hash,
{
    pub fn vote(&mut self, vote: T) {
        *self.0.entry(vote).or_default() += 1
    }
}

impl<D: Ord, T> Votes<D, T> {
    pub fn tally(&self) -> Tally<T>
    where
        T: Clone + Eq + Hash,
    {
        self.0.iter().fold(Tally::default(), |mut tally, (_, v)| {
            tally.vote(v.clone());
            tally
        })
    }
}
