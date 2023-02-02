use std::iter::repeat_with;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{post, put};
use axum::{Json, Router};
use radicle::crypto::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Duration, OffsetDateTime};

use crate::api::auth::{AuthState, DateTime, Session};
use crate::api::axum_extra::Path;
use crate::api::error::Error;
use crate::api::Context;

pub const UNAUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::seconds(60);
pub const AUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::weeks(1);

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/sessions", post(session_create_handler))
        .route(
            "/sessions/:id",
            put(session_signin_handler).get(session_handler),
        )
        .with_state(ctx)
}

#[derive(Debug, Deserialize, Serialize)]
struct AuthChallenge {
    sig: Signature,
    pk: PublicKey,
}

/// Create session.
/// `POST /sessions`
async fn session_create_handler(State(ctx): State<Context>) -> impl IntoResponse {
    let rng = fastrand::Rng::new();
    let session_id = repeat_with(|| rng.alphanumeric())
        .take(32)
        .collect::<String>();
    let signer = ctx.profile.signer().map_err(Error::from)?;
    let expiration_time = OffsetDateTime::now_utc()
        .checked_add(UNAUTHORIZED_SESSIONS_EXPIRATION)
        .unwrap();
    let session = Session {
        status: AuthState::Unauthorized,
        public_key: *signer.public_key(),
        issued_at: DateTime(OffsetDateTime::now_utc()),
        expires_at: DateTime(expiration_time),
    };
    let mut sessions = ctx.sessions.write().await;
    sessions.insert(session_id.clone(), session.clone());

    Ok::<_, Error>(Json(json!({
        "sessionId": session_id,
        "publicKey": session.public_key,
        "issuedAt": session.issued_at,
        "expiresAt": session.expires_at
    })))
}

/// Get a session.
/// `GET /sessions/:id`
async fn session_handler(
    State(ctx): State<Context>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let sessions = ctx.sessions.read().await;
    let session = sessions.get(&session_id).ok_or(Error::NotFound)?;

    Ok::<_, Error>(Json(json!({
        "status": session.status,
        "publicKey": session.public_key,
        "issuedAt": session.issued_at,
        "expiresAt": session.expires_at
    })))
}

/// Update session.
/// `PUT /sessions/:id`
async fn session_signin_handler(
    State(ctx): State<Context>,
    Path(session_id): Path<String>,
    Json(request): Json<AuthChallenge>,
) -> impl IntoResponse {
    let mut sessions = ctx.sessions.write().await;
    let session = sessions.get(&session_id).ok_or(Error::NotFound)?;
    if session.status == AuthState::Unauthorized {
        if session.public_key != request.pk {
            return Err(Error::Auth("Invalid public key"));
        }
        if session.expires_at <= DateTime(OffsetDateTime::now_utc()) {
            return Err(Error::Auth("Session expired"));
        }
        let payload = format!("{}:{}", session_id, request.pk);
        request
            .pk
            .verify(payload.as_bytes(), &request.sig)
            .map_err(Error::from)?;
        let expiration_time = OffsetDateTime::now_utc()
            .checked_add(AUTHORIZED_SESSIONS_EXPIRATION)
            .unwrap();
        let session = Session {
            status: AuthState::Authorized,
            public_key: request.pk,
            issued_at: session.issued_at.to_owned(),
            expires_at: DateTime(expiration_time),
        };
        sessions.insert(session_id.clone(), session);

        return Ok::<_, Error>(Json(json!({ "success": true })));
    }

    Err(Error::Auth("Session already authorized"))
}

#[cfg(test)]
mod routes {
    use axum::body::Body;
    use axum::http::StatusCode;
    use radicle_cli::commands::rad_web::{self, SessionInfo};

    use crate::api::test::{self, get, post, put};

    #[tokio::test]
    async fn test_session() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let app = super::router(ctx.to_owned());

        // Create session
        let response = post(&app, "/sessions", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = response.json().await;
        let session_info: SessionInfo = serde_json::from_value(json).unwrap();

        // Create request body
        let signer = ctx.profile.signer().unwrap();
        let signature = rad_web::sign(signer, &session_info).unwrap();
        let body = serde_json::to_vec(&super::AuthChallenge {
            sig: signature,
            pk: session_info.public_key,
        })
        .unwrap();

        let response = put(
            &app,
            format!("/sessions/{}", session_info.session_id),
            Some(Body::from(body)),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}
