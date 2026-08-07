//! ATProto `did:plc` identity support.
//!
//! Radicle keeps device transport IDs as Ed25519 `did:key` / [`crate::prelude::NodeId`].
//! Logical actors may additionally be `did:plc` identifiers whose verifying keys are
//! resolved via a hybrid model:
//!
//! - **Embedded pins** in the identity document (`xyz.radicle.did`) for authz-critical
//!   uses (delegates, private allow-lists).
//! - **On-disk PLC cache** (optionally refreshed from `plc.directory`) for authorship
//!   attribution.
//!
//! Only Ed25519 Multikey verification methods are accepted (`#atproto` preferred,
//! otherwise `#radicle`).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{PublicKey, PublicKeyError};
use crate::identity::Did;

/// Length of the identifier portion of a `did:plc` (base32lower, no padding).
pub const PLC_ID_LEN: usize = 24;

/// Validated `did:plc` identifier body (without the `did:plc:` prefix).
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
#[serde(transparent)]
pub struct PlcId([u8; PLC_ID_LEN]);

impl PlcId {
    /// Encode as the full DID string `did:plc:…`.
    pub fn encode(&self) -> String {
        format!("did:plc:{}", self.as_str())
    }

    /// Identifier body as a UTF-8 string.
    pub fn as_str(&self) -> &str {
        // SAFETY: bytes are validated as ASCII base32lower on construction.
        std::str::from_utf8(&self.0).expect("PlcId is valid UTF-8")
    }

    /// Decode a full `did:plc:…` string or bare identifier body.
    pub fn decode(input: &str) -> Result<Self, PlcIdError> {
        let body = input.strip_prefix("did:plc:").unwrap_or(input);
        if body.len() != PLC_ID_LEN {
            return Err(PlcIdError::Length(body.len()));
        }
        let mut bytes = [0u8; PLC_ID_LEN];
        for (i, b) in body.bytes().enumerate() {
            if !is_base32lower(b) {
                return Err(PlcIdError::Alphabet(b));
            }
            bytes[i] = b;
        }
        Ok(Self(bytes))
    }
}

fn is_base32lower(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'2'..=b'7')
}

impl fmt::Display for PlcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for PlcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PlcId({:?})", self.encode())
    }
}

impl FromStr for PlcId {
    type Err = PlcIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum PlcIdError {
    #[error("invalid did:plc length {0}, expected {PLC_ID_LEN}")]
    Length(usize),
    #[error("invalid did:plc character 0x{0:02x}, expected base32lower")]
    Alphabet(u8),
}

/// Subset of a DID document used by Radicle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidDocument {
    pub id: String,
    #[serde(default)]
    pub also_known_as: Vec<String>,
    #[serde(default)]
    pub verification_method: Vec<VerificationMethod>,
    #[serde(default)]
    pub service: Vec<serde_json::Value>,
}

/// Multikey verification method entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationMethod {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub controller: String,
    pub public_key_multibase: String,
}

impl DidDocument {
    /// Extract Ed25519 verifying keys accepted for Radicle.
    ///
    /// Prefers the first `#atproto` Multikey that is Ed25519; otherwise uses
    /// `#radicle`. Other key types are ignored.
    pub fn ed25519_keys(&self) -> Result<Vec<PublicKey>, DidDocumentError> {
        let mut atproto = Vec::new();
        let mut radicle = Vec::new();

        for method in &self.verification_method {
            if method.type_ != "Multikey" {
                continue;
            }
            if method.controller != self.id {
                continue;
            }
            let Ok(key) = PublicKey::from_str(&method.public_key_multibase) else {
                continue;
            };
            if method.id.ends_with("#atproto") {
                atproto.push(key);
            } else if method.id.ends_with("#radicle") {
                radicle.push(key);
            }
        }

        let keys = if !atproto.is_empty() {
            atproto
        } else {
            radicle
        };

        if keys.is_empty() {
            Err(DidDocumentError::NoEd25519Key)
        } else {
            Ok(keys)
        }
    }
}

#[derive(Error, Debug)]
pub enum DidDocumentError {
    #[error("DID document has no Ed25519 #atproto or #radicle Multikey")]
    NoEd25519Key,
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Resolved verifying material for a DID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDid {
    pub did: Did,
    pub keys: Vec<PublicKey>,
    pub also_known_as: Vec<String>,
}

/// Errors from DID resolution.
#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("DID document: {0}")]
    Document(#[from] DidDocumentError),
    #[error("PLC id: {0}")]
    PlcId(#[from] PlcIdError),
    #[error("public key: {0}")]
    PublicKey(#[from] PublicKeyError),
    #[error("not found in cache or embed: {0}")]
    NotFound(Did),
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Resolve logical DIDs to Ed25519 verifying keys.
pub trait DidResolver {
    fn resolve(&self, did: &Did) -> Result<ResolvedDid, ResolveError>;

    /// Map a device signing key to a logical DID when a PLC binding is known.
    fn actor_for_key(&self, key: &PublicKey) -> Did {
        Did::Key(*key)
    }
}

/// Resolver that only understands `did:key` (no PLC).
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyOnlyResolver;

impl DidResolver for KeyOnlyResolver {
    fn resolve(&self, did: &Did) -> Result<ResolvedDid, ResolveError> {
        match did {
            Did::Key(key) => Ok(ResolvedDid {
                did: Did::Key(*key),
                keys: vec![*key],
                also_known_as: vec![],
            }),
            Did::Plc(_) => Err(ResolveError::NotFound(*did)),
        }
    }
}

/// Pinned verifying keys for a DID, stored in the identity document payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedVerification {
    /// Ed25519 keys as multibase (`z6Mk…`) or `did:key:…` strings.
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plc_cid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}

impl PinnedVerification {
    pub fn from_keys(keys: impl IntoIterator<Item = PublicKey>) -> Self {
        Self {
            keys: keys.into_iter().map(|k| k.to_human()).collect(),
            plc_cid: None,
            as_of: None,
        }
    }

    pub fn public_keys(&self) -> Result<Vec<PublicKey>, PublicKeyError> {
        self.keys
            .iter()
            .map(|s| {
                let key = s.strip_prefix("did:key:").unwrap_or(s);
                PublicKey::from_str(key)
            })
            .collect()
    }
}

/// `xyz.radicle.did` payload: map of DID string → pinned verification.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DidPayload {
    pub pins: BTreeMap<String, PinnedVerification>,
}

impl DidPayload {
    pub fn get(&self, did: &Did) -> Option<&PinnedVerification> {
        self.pins.get(&did.encode())
    }

    pub fn insert(&mut self, did: Did, pin: PinnedVerification) {
        self.pins.insert(did.encode(), pin);
    }

    pub fn keys_for(&self, did: &Did) -> Result<Option<Vec<PublicKey>>, PublicKeyError> {
        match self.get(did) {
            Some(pin) => pin.public_keys().map(Some),
            None => Ok(None),
        }
    }

    /// Find a PLC DID that lists `key` among its pinned verifying keys.
    pub fn plc_for_key(&self, key: &PublicKey) -> Option<Did> {
        let human = key.to_human();
        let did_key = format!("did:key:{human}");
        for (id, pin) in &self.pins {
            if !id.starts_with("did:plc:") {
                continue;
            }
            if pin.keys.iter().any(|k| k == &human || k == &did_key) {
                if let Ok(did) = Did::decode(id) {
                    return Some(did);
                }
            }
        }
        None
    }
}

/// On-disk PLC DID document cache under `~/.radicle/cache/plc/`.
#[derive(Debug, Clone)]
pub struct PlcCache {
    path: PathBuf,
    directory: String,
}

impl PlcCache {
    pub const DEFAULT_DIRECTORY: &'static str = "https://plc.directory";

    pub fn new(home: impl AsRef<Path>) -> Self {
        Self {
            path: home.as_ref().join("cache").join("plc"),
            directory: Self::DEFAULT_DIRECTORY.to_owned(),
        }
    }

    pub fn with_directory(mut self, directory: impl Into<String>) -> Self {
        self.directory = directory.into();
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn file_for(&self, id: &PlcId) -> PathBuf {
        self.path.join(format!("{}.json", id.as_str()))
    }

    /// Load a cached DID document without network access.
    pub fn load(&self, id: &PlcId) -> Result<Option<DidDocument>, ResolveError> {
        let path = self.file_for(id);
        match fs::read_to_string(&path) {
            Ok(body) => Ok(Some(serde_json::from_str(&body)?)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Store a DID document in the cache.
    pub fn store(&self, id: &PlcId, doc: &DidDocument) -> Result<(), ResolveError> {
        fs::create_dir_all(&self.path)?;
        let path = self.file_for(id);
        let body = serde_json::to_string_pretty(doc)?;
        fs::write(path, body)?;
        Ok(())
    }

    /// Fetch from the PLC directory and update the cache.
    #[cfg(feature = "plc")]
    pub fn fetch(&self, id: &PlcId) -> Result<DidDocument, ResolveError> {
        let url = format!("{}/{}", self.directory.trim_end_matches('/'), id.encode());
        let body = ureq::get(&url)
            .call()
            .map_err(|e| ResolveError::Http(e.to_string()))?
            .into_string()
            .map_err(|e| ResolveError::Http(e.to_string()))?;
        let doc: DidDocument = serde_json::from_str(&body)?;
        if doc.id != id.encode() {
            return Err(ResolveError::Http(format!(
                "DID document id mismatch: got {}, expected {}",
                doc.id,
                id.encode()
            )));
        }
        self.store(id, &doc)?;
        Ok(doc)
    }

    /// Resolve from cache, optionally refreshing via network when `fetch` is true.
    pub fn resolve(&self, id: &PlcId, fetch: bool) -> Result<DidDocument, ResolveError> {
        if let Some(doc) = self.load(id)? {
            return Ok(doc);
        }
        #[cfg(feature = "plc")]
        if fetch {
            return self.fetch(id);
        }
        #[cfg(not(feature = "plc"))]
        let _ = fetch;
        Err(ResolveError::NotFound(Did::Plc(*id)))
    }
}

/// Hybrid resolver: identity-doc pins first, then PLC cache, then `did:key`.
#[derive(Debug, Clone)]
pub struct HybridResolver {
    cache: PlcCache,
    pins: DidPayload,
    /// Optional profile binding: local device key → PLC DID.
    bindings: BTreeMap<PublicKey, Did>,
    /// Attempt network fetch on cache miss when the `plc` feature is enabled.
    fetch: bool,
}

impl HybridResolver {
    pub fn new(cache: PlcCache) -> Self {
        Self {
            cache,
            pins: DidPayload::default(),
            bindings: BTreeMap::new(),
            fetch: false,
        }
    }

    pub fn with_pins(mut self, pins: DidPayload) -> Self {
        self.pins = pins;
        self
    }

    pub fn with_binding(mut self, key: PublicKey, did: Did) -> Self {
        self.bindings.insert(key, did);
        self
    }

    pub fn with_fetch(mut self, fetch: bool) -> Self {
        self.fetch = fetch;
        self
    }

    pub fn pins(&self) -> &DidPayload {
        &self.pins
    }

    pub fn pins_mut(&mut self) -> &mut DidPayload {
        &mut self.pins
    }

    pub fn cache(&self) -> &PlcCache {
        &self.cache
    }
}

impl DidResolver for HybridResolver {
    fn resolve(&self, did: &Did) -> Result<ResolvedDid, ResolveError> {
        match did {
            Did::Key(key) => Ok(ResolvedDid {
                did: Did::Key(*key),
                keys: vec![*key],
                also_known_as: vec![],
            }),
            Did::Plc(id) => {
                if let Some(keys) = self.pins.keys_for(did)? {
                    return Ok(ResolvedDid {
                        did: *did,
                        keys,
                        also_known_as: vec![],
                    });
                }
                let doc = self.cache.resolve(id, self.fetch)?;
                Ok(ResolvedDid {
                    did: *did,
                    keys: doc.ed25519_keys()?,
                    also_known_as: doc.also_known_as,
                })
            }
        }
    }

    fn actor_for_key(&self, key: &PublicKey) -> Did {
        if let Some(did) = self.bindings.get(key) {
            return *did;
        }
        if let Some(did) = self.pins.plc_for_key(key) {
            return did;
        }
        // Best-effort: scan cache directory for a document listing this key.
        if let Ok(entries) = fs::read_dir(self.cache.path()) {
            let human = key.to_human();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Ok(body) = fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(doc) = serde_json::from_str::<DidDocument>(&body) else {
                    continue;
                };
                if let Ok(keys) = doc.ed25519_keys() {
                    if keys.iter().any(|k| k.to_human() == human) {
                        if let Ok(plc) = PlcId::decode(&doc.id) {
                            return Did::Plc(plc);
                        }
                    }
                }
            }
        }
        Did::Key(*key)
    }
}

/// Helper: whether `key` may act for `did` given a resolver.
pub fn key_acts_for(resolver: &impl DidResolver, did: &Did, key: &PublicKey) -> bool {
    match resolver.resolve(did) {
        Ok(resolved) => resolved.keys.iter().any(|k| k == key),
        Err(_) => match did {
            Did::Key(k) => k == key,
            Did::Plc(_) => false,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;

    #[test]
    fn test_plc_id_roundtrip() {
        let input = "did:plc:ewvi7nxzyoun6grtyetllrat";
        let id = PlcId::decode(input).unwrap();
        assert_eq!(id.encode(), input);
        assert_eq!(id.as_str(), "ewvi7nxzyoun6grtyetllrat");
    }

    #[test]
    fn test_plc_id_rejects_bad_alphabet() {
        assert!(matches!(
            PlcId::decode("did:plc:ewvi7nxzyoun6grtyetllra0"),
            Err(PlcIdError::Alphabet(b'0'))
        ));
    }

    #[test]
    fn test_did_document_ed25519() {
        let key = PublicKey::from_str("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        let doc = DidDocument {
            id: "did:plc:ewvi7nxzyoun6grtyetllrat".into(),
            also_known_as: vec!["at://alice.test".into()],
            verification_method: vec![VerificationMethod {
                id: "did:plc:ewvi7nxzyoun6grtyetllrat#atproto".into(),
                type_: "Multikey".into(),
                controller: "did:plc:ewvi7nxzyoun6grtyetllrat".into(),
                public_key_multibase: key.to_human(),
            }],
            service: vec![],
        };
        assert_eq!(doc.ed25519_keys().unwrap(), vec![key]);
    }

    #[test]
    fn test_pinned_verification_roundtrip() {
        let key = PublicKey::from_str("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        let pin = PinnedVerification::from_keys([key]);
        assert_eq!(pin.public_keys().unwrap(), vec![key]);
    }
}
