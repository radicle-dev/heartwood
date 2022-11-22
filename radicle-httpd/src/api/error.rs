use axum::http;
use axum::response::{IntoResponse, Response};

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
    StorageError(#[from] radicle::storage::Error),
}

impl Error {
    pub fn status(&self) -> http::StatusCode {
        http::StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::error!("{}", self);

        self.status().into_response()
    }
}
