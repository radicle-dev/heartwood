mod delegates;
mod node;
mod projects;
mod sessions;
mod stats;

use axum::extract::State;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde_json::json;

use crate::api::{Context, VERSION};

pub fn router(ctx: Context) -> Router {
    let root_router = Router::new()
        .route("/", get(root_handler))
        .with_state(ctx.clone());

    let routes = Router::new()
        .merge(root_router)
        .merge(node::router(ctx.clone()))
        .merge(sessions::router(ctx.clone()))
        .merge(delegates::router(ctx.clone()))
        .merge(projects::router(ctx.clone()))
        .merge(stats::router(ctx));

    Router::new().nest("/v1", routes)
}

async fn root_handler(State(ctx): State<Context>) -> impl IntoResponse {
    let response = json!({
        "message": "Welcome!",
        "service": "radicle-httpd",
        "version": format!("{}-{}", VERSION, env!("GIT_HEAD")),
        "node": { "id": ctx.profile.public_key },
        "path": "/api/v1",
        "links": [
            {
                "href": "/projects",
                "rel": "projects",
                "type": "GET"
            },
            {
                "href": "/node",
                "rel": "node",
                "type": "GET"
            },
            {
                "href": "/delegates/:did/projects",
                "rel": "projects",
                "type": "GET"
            },
            {
                "href": "/stats",
                "rel": "stats",
                "type": "GET"
            }
        ]
    });

    Json(response)
}
