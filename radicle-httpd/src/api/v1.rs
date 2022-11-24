mod delegates;
mod node;
mod projects;
mod sessions;
mod stats;

use axum::Router;

use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    let routes = Router::new()
        .merge(node::router(ctx.clone()))
        .merge(sessions::router(ctx.clone()))
        .merge(delegates::router(ctx.clone()))
        .merge(projects::router(ctx.clone()))
        .merge(stats::router(ctx));

    Router::new().nest("/v1", routes)
}
