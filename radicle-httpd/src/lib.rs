#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
pub mod error;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::Command;
use std::str;
use std::sync::Arc;
use std::time::Duration;

use ::tracing::Span;
use anyhow::Context as _;
use axum::body::{Body, BoxBody, HttpBody};
use axum::http::{Request, Response};
use axum::middleware;
use axum::Router;
use radicle::identity::Id;
use tower_http::trace::TraceLayer;

use tracing_extra::{tracing_middleware, ColoredStatus, Paint, RequestId, TracingInfo};

mod api;
mod axum_extra;
mod git;
mod raw;
#[cfg(test)]
mod test;
mod tracing_extra;

#[derive(Debug, Clone)]
pub struct Options {
    pub aliases: HashMap<String, Id>,
    pub listen: SocketAddr,
}

/// Run the Server.
pub async fn run(options: Options) -> anyhow::Result<()> {
    let git_version = Command::new("git")
        .arg("version")
        .output()
        .context("'git' command must be available")?
        .stdout;

    tracing::info!("{}", str::from_utf8(&git_version)?.trim());

    let profile = Arc::new(radicle::Profile::load()?);
    let request_id = RequestId::new();

    tracing::info!("using radicle home at {}", profile.home().display());

    let ctx = api::Context::new(profile.clone());
    let api_router = api::router(ctx);
    let git_router = git::router(profile.clone(), options.aliases);
    let raw_router = raw::router(profile);

    tracing::info!("listening on http://{}", options.listen);

    let app = Router::new()
        .merge(git_router)
        .nest("/api", api_router)
        .nest("/raw", raw_router)
        .layer(middleware::from_fn(tracing_middleware))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(move |_request: &Request<Body>| {
                    tracing::info_span!("request", id = %request_id.clone().next())
                })
                .on_response(
                    |response: &Response<BoxBody>, latency: Duration, _span: &Span| {
                        if let Some(info) = response.extensions().get::<TracingInfo>() {
                            tracing::info!(
                                "{} \"{} {} {:?}\" {} {:?} {}",
                                info.connect_info.0,
                                info.method,
                                info.uri,
                                info.version,
                                ColoredStatus(response.status()),
                                latency,
                                Paint::dim(
                                    response
                                        .body()
                                        .size_hint()
                                        .exact()
                                        .map(|n| n.to_string())
                                        .unwrap_or("0".to_string())
                                        .into()
                                ),
                            );
                        } else {
                            tracing::info!("Processed");
                        }
                    },
                ),
        )
        .into_make_service_with_connect_info::<SocketAddr>();

    axum::Server::bind(&options.listen)
        .serve(app)
        .await
        .map_err(anyhow::Error::from)
}
