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

    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid identifier.
    #[error("invalid radicle identifier: {0}")]
    Id(#[from] radicle::identity::IdError),

    /// HeaderName error.
    #[error(transparent)]
    InvalidHeaderName(#[from] axum::http::header::InvalidHeaderName),

    /// HeaderValue error.
    #[error(transparent)]
    InvalidHeaderValue(#[from] axum::http::header::InvalidHeaderValue),

    /// An error occurred while verifying the siwe message.
    #[error(transparent)]
    SiweVerification(#[from] siwe::VerificationError),

    /// An error occurred while parsing the siwe message.
    #[error(transparent)]
    SiweParse(#[from] siwe::ParseError),

    /// Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),

    /// Cob store error.
    #[error(transparent)]
    CobStore(#[from] radicle::cob::store::Error),

    /// Git project error.
    #[error(transparent)]
    GitProject(#[from] radicle::storage::git::ProjectError),

    /// Surf commit error.
    #[error(transparent)]
    SurfCommit(#[from] radicle_surf::commit::Error),

    /// Surf object error.
    #[error(transparent)]
    SurfObject(#[from] radicle_surf::object::Error),

    /// Surf git error.
    #[error(transparent)]
    SurfGit(#[from] radicle_surf::git::Error),

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
            Error::SiweParse(msg) => (StatusCode::BAD_REQUEST, Some(msg.to_string())),
            Error::SiweVerification(msg) => (StatusCode::BAD_REQUEST, Some(msg.to_string())),
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
