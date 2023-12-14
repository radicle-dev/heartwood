use std::fmt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Extension;
use hyper::{Method, StatusCode, Uri, Version};

pub use radicle_term::ansi::Paint;

#[derive(Clone)]
pub struct RequestId(Arc<AtomicU64>);

impl RequestId {
    pub fn new() -> RequestId {
        RequestId(Arc::new(0.into()))
    }

    pub fn next(&mut self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct TracingInfo {
    pub connect_info: ConnectInfo<SocketAddr>,
    pub method: Method,
    pub version: Version,
    pub uri: Uri,
}

pub struct ColoredStatus(pub StatusCode);

impl fmt::Display for ColoredStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.as_u16() {
            200..=299 => write!(f, "{}", Paint::green(self.0)),
            300..=399 => write!(f, "{}", Paint::blue(self.0)),
            400..=499 => write!(f, "{}", Paint::red(self.0)),
            _ => write!(f, "{}", Paint::yellow(self.0)),
        }
    }
}

pub async fn tracing_middleware(request: Request<Body>, next: Next) -> impl IntoResponse {
    let connect_info = *request
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .unwrap();

    let method = request.method().clone();
    let version = request.version();
    let uri = request.uri().clone();

    let tracing_info = TracingInfo {
        connect_info,
        method,
        version,
        uri,
    };

    let response = next.run(request).await;

    (Extension(tracing_info), response)
}
