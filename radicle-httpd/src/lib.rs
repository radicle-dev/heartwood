#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![recursion_limit = "256"]
pub mod error;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::process::Command;
use std::str;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use axum::body::{Body, BoxBody, HttpBody};
use axum::http::{Request, Response};
use axum::middleware;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing::Span;

use radicle::identity::Id;
use radicle::Profile;

use tracing_extra::{tracing_middleware, ColoredStatus, Paint, RequestId, TracingInfo};

mod api;
mod axum_extra;
mod cache;
mod git;
mod raw;
#[cfg(test)]
mod test;
mod tracing_extra;

/// Default cache HTTP size.
pub const DEFAULT_CACHE_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(100) };

#[derive(Debug, Clone)]
pub struct Options {
    pub aliases: HashMap<String, Id>,
    pub listen: SocketAddr,
    pub cache: Option<NonZeroUsize>,
}

/// Run the Server.
pub async fn run(options: Options) -> anyhow::Result<()> {
    let git_version = Command::new("git")
        .arg("version")
        .output()
        .context("'git' command must be available")?
        .stdout;

    tracing::info!("{}", str::from_utf8(&git_version)?.trim());

    let listen = options.listen;

    tracing::info!("listening on http://{}", listen);

    let profile = Profile::load()?;
    let request_id = RequestId::new();

    tracing::info!("using radicle home at {}", profile.home().path().display());

    let app =
        router(options, profile)?
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

    axum::Server::bind(&listen)
        .serve(app)
        .await
        .map_err(anyhow::Error::from)
}

/// Create a router consisting of other sub-routers.
fn router(options: Options, profile: Profile) -> anyhow::Result<Router> {
    let profile = Arc::new(profile);
    let ctx = api::Context::new(profile.clone(), &options);

    let api_router = api::router(ctx);
    let git_router = git::router(profile.clone(), options.aliases);
    let raw_router = raw::router(profile);

    let app = Router::new()
        .merge(git_router)
        .nest("/api", api_router)
        .nest("/raw", raw_router);

    Ok(app)
}

pub mod logger {
    use tracing::dispatcher::Dispatch;

    pub fn init() -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
        tracing::dispatcher::set_global_default(Dispatch::new(subscriber()))
    }

    #[cfg(feature = "logfmt")]
    pub fn subscriber() -> impl tracing::Subscriber {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::EnvFilter;

        tracing_subscriber::Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(tracing_logfmt::layer())
    }

    #[cfg(not(feature = "logfmt"))]
    pub fn subscriber() -> impl tracing::Subscriber {
        tracing_subscriber::FmtSubscriber::builder()
            .with_target(false)
            .with_max_level(tracing::Level::DEBUG)
            .finish()
    }
}

#[cfg(test)]
mod routes {
    use std::collections::HashMap;
    use std::net::SocketAddr;

    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::StatusCode;

    use crate::test::{self, get};

    #[tokio::test]
    async fn test_invalid_route_returns_404() {
        let tmp = tempfile::tempdir().unwrap();
        let app = super::router(
            super::Options {
                aliases: HashMap::new(),
                listen: SocketAddr::from(([0, 0, 0, 0], 8080)),
                cache: None,
            },
            test::profile(tmp.path(), [0xff; 32]),
        )
        .unwrap()
        .layer(MockConnectInfo(SocketAddr::from(([0, 0, 0, 0], 8080))));

        let response = get(&app, "/aa/a").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
