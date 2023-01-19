use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Errors relating to the HTTP backend.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The entity was not found.
    #[error("entity not found")]
    NotFound,

    /// An error occurred during an authentication process.
    #[error("could not authenticate: {0}")]
    Auth(&'static str),

    /// An error occurred with env variables.
    #[error(transparent)]
    Env(#[from] std::env::VarError),

    /// Profile error.
    #[error(transparent)]
    Profile(#[from] radicle::profile::Error),

    /// Crypto error.
    #[error(transparent)]
    Crypto(#[from] radicle::crypto::Error),

    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),

    /// Cob issue error.
    #[error(transparent)]
    CobIssue(#[from] radicle::cob::issue::Error),

    /// Cob store error.
    #[error(transparent)]
    CobStore(#[from] radicle::cob::store::Error),

    /// Git project error.
    #[error(transparent)]
    GitProject(#[from] radicle::storage::git::ProjectError),

    /// Surf directory error.
    #[error(transparent)]
    SurfDir(#[from] radicle_surf::fs::error::Directory),

    /// Surf error.
    #[error(transparent)]
    Surf(#[from] radicle_surf::Error),

    /// Git2 error.
    #[error(transparent)]
    Git2(#[from] radicle::git::raw::Error),

    /// Storage refs error.
    #[error(transparent)]
    StorageRef(#[from] radicle::storage::refs::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            Error::NotFound => (StatusCode::NOT_FOUND, None),
            Error::Auth(msg) => (StatusCode::BAD_REQUEST, Some(msg.to_string())),
            Error::Crypto(msg) => (StatusCode::BAD_REQUEST, Some(msg.to_string())),
            Error::Git2(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Some(e.message().to_owned()),
            ),
            _ => {
                tracing::error!("Error: {:?}", &self);

                (StatusCode::INTERNAL_SERVER_ERROR, None)
            }
        };

        let body = Json(json!({
            "error": msg.or_else(|| status.canonical_reason().map(|r| r.to_string())),
            "code": status.as_u16()
        }));

        (status, body).into_response()
    }
}
