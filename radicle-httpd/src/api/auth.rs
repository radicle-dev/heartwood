use radicle::crypto::PublicKey;
use serde::{Serialize, Serializer};
use time::OffsetDateTime;

#[derive(Clone, PartialEq, PartialOrd)]
pub struct DateTime(pub OffsetDateTime);

impl Serialize for DateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}", self.0))
    }
}

#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthState {
    Authorized,
    Unauthorized,
}

#[derive(Clone)]
pub struct Session {
    pub status: AuthState,
    pub public_key: PublicKey,
    pub issued_at: DateTime,
    pub expires_at: DateTime,
}
