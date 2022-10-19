use axum::http;
use axum::response::{IntoResponse, Response};

/// Errors relating to the HTTP backend.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
    #[error("backend error")]
    Backend,

    /// Id is not valid.
    #[error("id is not valid")]
    InvalidId,

    /// HeaderName error.
    #[error(transparent)]
    InvalidHeaderName(#[from] axum::http::header::InvalidHeaderName),

    /// HeaderValue error.
    #[error(transparent)]
    InvalidHeaderValue(#[from] axum::http::header::InvalidHeaderValue),
}

impl Error {
    pub fn status(&self) -> http::StatusCode {
        match self {
            Error::ServiceUnavailable(_) => http::StatusCode::SERVICE_UNAVAILABLE,
            Error::InvalidId => http::StatusCode::NOT_FOUND,
            _ => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::error!("{}", self);

        self.status().into_response()
    }
}
