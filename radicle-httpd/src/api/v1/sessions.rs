use std::collections::HashMap;
use std::convert::TryInto;
use std::env;
use std::iter::repeat_with;
use std::str::FromStr;

use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use ethers_core::utils::hex;
use hyper::http::uri::Authority;
use serde_json::json;
use siwe::Message;
use time::{Duration, OffsetDateTime};

use crate::api::auth::{AuthRequest, AuthState, DateTime, Session};
use crate::api::axum_extra::Path;
use crate::api::error::Error;
use crate::api::Context;

pub const UNAUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::seconds(60);

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/sessions", post(session_create_handler))
        .route(
            "/sessions/:id",
            get(session_get_handler).put(session_signin_handler),
        )
        .layer(Extension(ctx))
}

/// Create session.
/// `POST /sessions`
async fn session_create_handler(Extension(ctx): Extension<Context>) -> impl IntoResponse {
    let expiration_time = OffsetDateTime::now_utc()
        .checked_add(UNAUTHORIZED_SESSIONS_EXPIRATION)
        .unwrap();
    let mut sessions = ctx.sessions.write().await;
    let (session_id, nonce) = create_session(&mut sessions, DateTime(expiration_time));

    Json(json!({ "id": session_id, "nonce": nonce }))
}

/// Get session.
/// `GET /sessions/:id`
async fn session_get_handler(
    Extension(ctx): Extension<Context>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let sessions = ctx.sessions.read().await;
    let session = sessions.get(&id).ok_or(Error::NotFound)?;

    match session {
        AuthState::Authorized(session) => {
            Ok::<_, Error>(Json(json!({ "id": id, "session": session })))
        }
        AuthState::Unauthorized {
            nonce,
            expiration_time,
        } => Ok::<_, Error>(Json(
            json!({ "id": id, "nonce": nonce, "expirationTime": expiration_time }),
        )),
    }
}

/// Update session.
/// `PUT /sessions/:id`
async fn session_signin_handler(
    Extension(ctx): Extension<Context>,
    Path(id): Path<String>,
    Json(request): Json<AuthRequest>,
) -> impl IntoResponse {
    // Get unauthenticated session data, return early if not found
    let mut sessions = ctx.sessions.write().await;
    let session = sessions.get(&id).ok_or(Error::NotFound)?;

    if let AuthState::Unauthorized { nonce, .. } = session {
        let message = Message::from_str(request.message.as_str()).map_err(Error::from)?;

        let host = env::var("RADICLE_DOMAIN").map_err(Error::from)?;

        // Validate nonce
        if *nonce != message.nonce {
            return Err(Error::Auth("Invalid nonce"));
        }

        // Verify that domain is the correct one
        let authority = Authority::from_str(&host).map_err(|_| Error::Auth("Invalid host"))?;
        if authority != message.domain {
            return Err(Error::Auth("Invalid domain"));
        }

        // Verifies the following:
        // - AuthRequest sig matches the address passed in the AuthRequest message.
        // - expirationTime is not in the past.
        // - notBefore time is in the future.
        message
            .verify(&request.signature.to_vec(), &Default::default())
            .await
            .map_err(Error::from)?;

        let session: Session = message.try_into()?;
        sessions.insert(id.clone(), AuthState::Authorized(session.clone()));

        return Ok::<_, Error>(Json(json!({ "id": id, "session": session })));
    }

    Err(Error::Auth("Session already authorized"))
}

fn create_session(
    map: &mut HashMap<String, AuthState>,
    expiration_time: DateTime,
) -> (String, String) {
    let nonce = siwe::generate_nonce();

    // We generate a value from the RNG for the session id
    let rng = fastrand::Rng::new();
    let id = hex::encode(repeat_with(|| rng.u8(..)).take(32).collect::<Vec<u8>>());

    let auth_state = AuthState::Unauthorized {
        nonce: nonce.clone(),
        expiration_time,
    };

    map.insert(id.clone(), auth_state);

    (id, nonce)
}
