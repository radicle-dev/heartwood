use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, put};
use axum::{Json, Router};
use axum_auth::AuthBearer;
use hyper::StatusCode;
use serde_json::json;

use radicle::identity::Id;
use radicle::node::{policy, Handle, DEFAULT_TIMEOUT};
use radicle::Node;

use crate::api::error::Error;
use crate::api::{self, Context, PoliciesQuery, VERSION};
use crate::axum_extra::{Path, Query};

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/node", get(node_handler))
        .route("/node/policies/repos", get(node_policies_repos_handler))
        .route(
            "/node/policies/repos/:rid",
            put(node_policies_seed_handler).delete(node_policies_unseed_handler),
        )
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
        "version": format!("{}-{}", VERSION, env!("GIT_HEAD")),
        "config": config,
        "state": node_state,
    });

    Ok::<_, Error>(Json(response))
}

/// Return local repo policies information.
/// `GET /node/policies/repos`
async fn node_policies_repos_handler(State(ctx): State<Context>) -> impl IntoResponse {
    let policies = ctx.profile.policies()?;
    let mut repos = Vec::new();

    for policy::Repo { id, scope, policy } in policies.seed_policies()? {
        repos.push(json!({
            "id": id,
            "scope": scope,
            "policy": policy,
        }));
    }

    Ok::<_, Error>(Json(repos))
}

/// Seed a new repo.
/// `PUT /node/policies/repos/:rid`
async fn node_policies_seed_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(project): Path<Id>,
    Query(qs): Query<PoliciesQuery>,
) -> impl IntoResponse {
    api::auth::validate(&ctx, &token).await?;
    let mut node = Node::new(ctx.profile.socket());
    node.seed(project, qs.scope.unwrap_or_default())?;

    if let Some(from) = qs.from {
        let results = node.fetch(project, from, DEFAULT_TIMEOUT)?;
        return Ok::<_, Error>((
            StatusCode::OK,
            Json(json!({ "success": true, "results": results })),
        ));
    }
    Ok::<_, Error>((StatusCode::OK, Json(json!({ "success": true }))))
}

/// Unseed a repo.
/// `DELETE /node/policies/repos/:rid`
async fn node_policies_unseed_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(project): Path<Id>,
) -> impl IntoResponse {
    api::auth::validate(&ctx, &token).await?;
    let mut node = Node::new(ctx.profile.socket());
    node.unseed(project)?;

    Ok::<_, Error>((StatusCode::OK, Json(json!({ "success": true }))))
}
