use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use radicle::node::NodeId;

use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    let node_id = ctx.profile.public_key;

    Router::new()
        .route("/node", get(node_handler))
        .with_state(node_id)
}

/// Return the node id for the node identity.
/// `GET /node`
async fn node_handler(State(node_id): State<NodeId>) -> impl IntoResponse {
    let response = json!({
        "id": node_id.to_string(),
    });

    Json(response)
}
