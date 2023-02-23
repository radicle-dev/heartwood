use axum::extract::path::ErrorKind;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::{async_trait, Json};

use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct Path<T>(pub T);

#[async_trait]
impl<S, T> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<Error>);

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(req, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                let status = StatusCode::BAD_REQUEST;
                let body = match rejection {
                    PathRejection::FailedToDeserializePathParams(inner) => {
                        let kind = inner.into_kind();
                        match &kind {
                            ErrorKind::Message(msg) => Json(Error {
                                success: false,
                                error: msg.to_string(),
                            }),
                            _ => Json(Error {
                                success: false,
                                error: kind.to_string(),
                            }),
                        }
                    }
                    _ => Json(Error {
                        success: false,
                        error: format!("{rejection}"),
                    }),
                };

                Err((status, body))
            }
        }
    }
}

#[derive(Default)]
pub struct Query<T>(pub T);

#[async_trait]
impl<S, T> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<Error>);

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(req, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                let status = StatusCode::BAD_REQUEST;
                let body = match rejection {
                    QueryRejection::FailedToDeserializeQueryString(inner) => Json(Error {
                        success: false,
                        error: inner.to_string(),
                    }),
                    _ => Json(Error {
                        success: false,
                        error: format!("{rejection}"),
                    }),
                };

                Err((status, body))
            }
        }
    }
}

#[derive(Serialize)]
pub struct Error {
    success: bool,
    error: String,
}
