use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use radicle::node::Handle;
use radicle::Node;

use crate::api::error::Error;
use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/node", get(node_handler))
        .with_state(ctx)
}

/// Return local node information.
/// `GET /node`
async fn node_handler(State(ctx): State<Context>) -> impl IntoResponse {
    let node = Node::new(ctx.profile.socket());
    let node_id = ctx.profile.public_key;
    let node_state = if node.is_running() {
        "running"
    } else {
        "stopped"
    };
    let config = node.config()?;
    let response = json!({
        "id": node_id.to_string(),
        "config": config,
        "state": node_state,
    });

    Ok::<_, Error>(Json(response))
}
