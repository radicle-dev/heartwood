use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde_json::json;

use crate::api::error::Error;
use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/stats", get(stats_handler))
        .layer(Extension(ctx))
}

/// Return the stats for the node.
/// `GET /stats`
async fn stats_handler(Extension(ctx): Extension<Context>) -> impl IntoResponse {
    let storage = &ctx.profile.storage;
    let projects = storage.projects()?.len();

    Ok::<_, Error>(Json(
        json!({ "projects": { "count": projects }, "users": { "count": 0 } }),
    ))
}

#[cfg(test)]
mod routes {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::api::test;

    #[tokio::test]
    async fn test_stats() {
        let app = super::router(test::seed());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/stats".to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            body,
            json!({
                "projects": {
                    "count": 1
                },
                "users": {
                    "count": 0
                }
            })
        );
    }
}
