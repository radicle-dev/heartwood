use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::api::error::Error;
use crate::api::Context;

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/profile", get(profile_handler))
        .with_state(ctx)
}

/// Return local profile information.
/// `GET /profile`
async fn profile_handler(
    State(ctx): State<Context>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if !addr.ip().is_loopback() {
        return Err(Error::Auth("Profile data is only shown for localhost"));
    }

    Ok::<_, Error>(Json(
        json!({ "config": ctx.profile.config, "home": ctx.profile.home.path() }),
    ))
}

#[cfg(test)]
mod routes {
    use std::net::SocketAddr;

    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::test::{self, get};

    #[tokio::test]
    async fn test_remote_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = test::seed(tmp.path());
        let app = super::router(seed.clone())
            .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 1], 8080))));
        let response = get(&app, "/profile").await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.json().await,
            json!({
              "error": "Profile data is only shown for localhost",
              "code": 401
            })
        )
    }

    #[tokio::test]
    async fn test_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = test::seed(tmp.path());
        let app = super::router(seed.clone())
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        let response = get(&app, "/profile").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json().await,
            json!({
              "config": {
                "publicExplorer": "https://app.radicle.xyz/nodes/$host/$rid",
                "preferredSeeds": [
                  "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776"
                ],
                "cli": {
                  "hints": true
                },
                "node": {
                  "alias": "seed",
                  "listen": [],
                  "peers": {
                    "type": "dynamic",
                    "target": 8
                  },
                  "connect": [],
                  "externalAddresses": [],
                  "network": "main",
                  "relay": true,
                  "limits": {
                    "routingMaxSize": 1000,
                    "routingMaxAge": 604800,
                    "gossipMaxAge": 1209600,
                    "fetchConcurrency": 1,
                    "rate": {
                      "inbound": {
                        "fillRate": 0.2,
                        "capacity": 32
                      },
                      "outbound": {
                        "fillRate": 1.0,
                        "capacity": 64
                      }
                    }
                  },
                  "policy": "block",
                  "scope": "followed"
                }
              },
              "home": seed.profile.path()
            })
        );
    }
}
