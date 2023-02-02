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

#[derive(Clone)]
pub enum AuthState {
    Authorized(Session),
    Unauthorized(Session),
}

#[derive(Clone)]
pub struct Session {
    pub status: String,
    pub public_key: PublicKey,
    pub issued_at: DateTime,
    pub expires_at: DateTime,
}

impl From<AuthState> for Session {
    fn from(other: AuthState) -> Self {
        match other {
            AuthState::Authorized(s) => s,
            AuthState::Unauthorized(s) => s,
        }
    }
}
