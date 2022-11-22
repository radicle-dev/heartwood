mod node;

use axum::Router;

use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    let routes = Router::new().merge(node::router(ctx));

    Router::new().nest("/v1", routes)
}
