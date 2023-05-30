use serde::{Deserialize, Serialize};
use time::serde::timestamp;
use time::{Duration, OffsetDateTime};

use radicle::crypto::PublicKey;

use crate::api::error::Error;
use crate::api::Context;

pub const UNAUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::seconds(60);
pub const AUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::weeks(1);

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

pub async fn validate(ctx: &Context, token: &str) -> Result<(), Error> {
    let sessions_store = ctx.sessions.read().await;
    let session = sessions_store
        .get(token)
        .ok_or(Error::Auth("Unauthorized"))?;

    if session.status != AuthState::Authorized || session.expires_at <= OffsetDateTime::now_utc() {
        return Err(Error::Auth("Unauthorized"));
    }

    Ok(())
}
