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
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::api::test::{self, request};

    #[tokio::test]
    async fn test_stats() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(test::seed(tmp.path()));
        let response = request(&app, "/stats").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
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
