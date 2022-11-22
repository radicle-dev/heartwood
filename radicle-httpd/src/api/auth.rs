use std::convert::TryFrom;
use std::str::FromStr;

use ethers_core::types::{Signature, H160};
use serde::{Deserialize, Serialize, Serializer};
use time::OffsetDateTime;

use crate::error::Error;

#[derive(Clone)]
pub struct DateTime(OffsetDateTime);

impl Serialize for DateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}", self.0))
    }
}

#[derive(Deserialize, Serialize)]
pub struct AuthRequest {
    pub message: String,
    #[serde(deserialize_with = "deserialize_signature")]
    pub signature: Signature,
}

fn deserialize_signature<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    Signature::from_str(&buf).map_err(serde::de::Error::custom)
}

pub enum AuthState {
    Authorized(Session),
    Unauthorized {
        nonce: String,
        expiration_time: DateTime,
    },
}

// We copy the implementation of siwe::Message here to derive Serialization and Debug
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub domain: String,
    pub address: H160,
    pub statement: Option<String>,
    pub uri: String,
    pub version: u64,
    pub chain_id: u64,
    pub nonce: String,
    pub issued_at: DateTime,
    pub expiration_time: Option<DateTime>,
    pub resources: Vec<String>,
}

impl TryFrom<siwe::Message> for Session {
    type Error = Error;

    fn try_from(message: siwe::Message) -> Result<Session, Error> {
        Ok(Session {
            domain: message.domain.host().to_string(),
            address: H160(message.address),
            statement: None,
            uri: message.uri.to_string(),
            version: message.version as u64,
            chain_id: message.chain_id,
            nonce: message.nonce,
            issued_at: DateTime(message.issued_at.as_ref().to_owned()),
            expiration_time: message
                .expiration_time
                .map(|x| DateTime(x.as_ref().to_owned())),
            resources: message.resources.iter().map(|r| r.to_string()).collect(),
        })
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_auth_request_de() {
        let json = serde_json::json!({
            "message": "Hello World!",
            "signature": "20096c6ed2bcccb88c9cafbbbbda7a5a3cff6d0ca318c07faa58464083ca40a92f899fbeb26a4c763a7004b13fd0f1ba6c321d4e3a023e30f63c40d4154b99a41c"
        });

        let _req: super::AuthRequest = serde_json::from_value(json).unwrap();
    }
}
