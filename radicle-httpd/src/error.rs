use std::process::ExitStatus;

use axum::http;
use axum::response::{IntoResponse, Response};

/// Errors relating to the Git backend.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The entity was not found.
    #[error("not found")]
    NotFound,

    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// The service is not available.
    #[error("service '{0}' not available")]
    ServiceUnavailable(&'static str),

    /// Invalid identifier.
    #[error("invalid radicle identifier: {0}")]
    Id(#[from] radicle::identity::IdError),

    /// Git backend error.
    #[error("git-http-backend: exited with code {0}")]
    BackendExited(ExitStatus),

    /// Git backend error.
    #[error("git-http-backend: invalid header returned: {0:?}")]
    BackendHeader(String),

    /// HeaderName error.
    #[error(transparent)]
    InvalidHeaderName(#[from] axum::http::header::InvalidHeaderName),

    /// HeaderValue error.
    #[error(transparent)]
    InvalidHeaderValue(#[from] axum::http::header::InvalidHeaderValue),
}

impl GitError {
    pub fn status(&self) -> http::StatusCode {
        match self {
            GitError::ServiceUnavailable(_) => http::StatusCode::SERVICE_UNAVAILABLE,
            GitError::Id(_) => http::StatusCode::NOT_FOUND,
            GitError::NotFound => http::StatusCode::NOT_FOUND,
            _ => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for GitError {
    fn into_response(self) -> Response {
        tracing::error!("{}", self);

        self.status().into_response()
    }
}

/// Errors relating to the `/raw` route.
#[derive(Debug, thiserror::Error)]
pub enum RawError {
    /// Surf error.
    #[error(transparent)]
    Surf(#[from] radicle_surf::Error),

    /// Git error.
    #[error(transparent)]
    Git(#[from] radicle::git::ext::Error),

    /// Radicle Storage error.
    #[error(transparent)]
    Storage(#[from] radicle::storage::Error),

    /// Http Headers error.
    #[error(transparent)]
    Headers(#[from] http::header::InvalidHeaderValue),

    /// Surf file error.
    #[error(transparent)]
    SurfFile(#[from] radicle_surf::fs::error::File),
}

impl RawError {
    pub fn status(&self) -> http::StatusCode {
        match self {
            RawError::SurfFile(_) => http::StatusCode::NOT_FOUND,
            _ => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for RawError {
    fn into_response(self) -> Response {
        tracing::error!("{}", self);

        self.status().into_response()
    }
}
