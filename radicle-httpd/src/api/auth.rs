use radicle::crypto::PublicKey;
use serde::{Deserialize, Serialize};
use time::{serde::timestamp, OffsetDateTime};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthState {
    Authorized,
    Unauthorized,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub status: AuthState,
    pub public_key: PublicKey,
    #[serde(with = "timestamp")]
    pub issued_at: OffsetDateTime,
    #[serde(with = "timestamp")]
    pub expires_at: OffsetDateTime,
}
