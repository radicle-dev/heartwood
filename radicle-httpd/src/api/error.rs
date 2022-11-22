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
