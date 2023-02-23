#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
pub mod error;

use std::net::SocketAddr;
use std::process::Command;
use std::str;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use axum::body::{Body, BoxBody};
use axum::http::{Request, Response};
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing::Span;

mod api;
mod axum_extra;
mod git;
mod raw;
#[cfg(test)]
mod test;

#[derive(Debug, Clone)]
pub struct Options {
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

    tracing::info!("using radicle home at {}", profile.home().display());

    let ctx = api::Context::new(profile.clone());
    let api_router = api::router(ctx);
    let git_router = git::router(profile.clone());
    let raw_router = raw::router(profile);

    tracing::info!("listening on http://{}", options.listen);

    let app = Router::new()
        .merge(git_router)
        .nest("/api", api_router)
        .nest("/raw", raw_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    tracing::info_span!(
                        "request",
                        method = %request.method(),
                        uri = %request.uri(),
                        status = tracing::field::Empty,
                        latency = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &Response<BoxBody>, latency: Duration, span: &Span| {
                        span.record("status", &tracing::field::debug(response.status()));
                        span.record("latency", &tracing::field::debug(latency));

                        tracing::info!("Processed");
                    },
                ),
        )
        .into_make_service_with_connect_info::<SocketAddr>();

    axum::Server::bind(&options.listen)
        .serve(app)
        .await
        .map_err(anyhow::Error::from)
}
