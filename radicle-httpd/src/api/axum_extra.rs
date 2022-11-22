use axum::extract::path::ErrorKind;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{FromRequest, RequestParts};
use axum::http::StatusCode;
use axum::{async_trait, Json};

use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct Path<T>(pub T);

#[async_trait]
impl<B, T> FromRequest<B> for Path<T>
where
    T: DeserializeOwned + Send,
    B: Send,
{
    type Rejection = (StatusCode, axum::Json<Error>);

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request(req).await {
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
                        error: format!("{}", rejection),
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
impl<B, T> FromRequest<B> for Query<T>
where
    T: DeserializeOwned + Send,
    B: Send,
{
    type Rejection = (StatusCode, axum::Json<Error>);

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request(req).await {
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
                        error: format!("{}", rejection),
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
