use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use radicle::node::{tracking, Handle};
use radicle::Node;

use crate::api::error::Error;
use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/node", get(node_handler))
        .route("/node/tracking/repos", get(node_tracking_repos_handler))
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
    let config = match node.config() {
        Ok(config) => Some(config),
        Err(err) => {
            tracing::error!("Error getting node config: {:#}", err);
            None
        }
    };
    let response = json!({
        "id": node_id.to_string(),
        "config": config,
        "state": node_state,
    });

    Ok::<_, Error>(Json(response))
}

/// Return local tracking repos information.
/// `GET /node/tracking/repos`
async fn node_tracking_repos_handler(State(ctx): State<Context>) -> impl IntoResponse {
    let tracking = ctx.profile.tracking()?;
    let mut repos = Vec::new();

    for tracking::Repo { id, scope, policy } in tracking.repo_policies()? {
        repos.push(json!({
            "id": id,
            "scope": scope,
            "policy": policy,
        }));
    }

    Ok::<_, Error>(Json(repos))
}
