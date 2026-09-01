//! Identity extraction from trusted proxy headers.
//!
//! Catalog does not talk to Identity or validate sessions/JWTs itself.
//! Per the container diagram (SDD §3.1) requests reach this service
//! through the API Gateway, which — mirroring the pattern already used
//! by `authnz`'s own reverse proxy (`authnz::proxy`) — terminates the
//! session/JWT, strips any client-supplied copies of these headers, and
//! sets `X-User-Id` / `X-User-Email` / `X-User-Name` itself from the
//! authenticated session before forwarding upstream.
//!
//! **Deployment requirement:** this service MUST NOT be reachable except
//! through that gateway. Anything that can reach catalog directly can
//! forge these headers and impersonate any user. This mirrors the same
//! assumption `authnz::proxy` already makes for whatever it fronts.

use actix_web::dev::Payload;
use actix_web::{Error as ActixError, FromRequest, HttpRequest, error};
use std::future::{Ready, ready};
use uuid::Uuid;

/// The authenticated caller, trusted from gateway-set headers.
///
/// Use this extractor on any route that requires a logged-in user
/// (publish, update metadata, yank/un-yank, delete). Missing or
/// unparseable headers are rejected with `401`.
#[derive(Debug, Clone)]
pub struct Identity {
    pub user_id: Uuid,
    pub email: String,
    pub username: String,
}

fn extract(req: &HttpRequest) -> Result<Identity, ActixError> {
    let header_str = |name: &str| -> Option<String> {
        req.headers().get(name)?.to_str().ok().map(str::to_string)
    };

    let user_id = header_str("X-User-Id")
        .and_then(|v| Uuid::parse_str(&v).ok())
        .ok_or_else(|| {
            error::ErrorUnauthorized(
                "missing or invalid X-User-Id — request did not come through the gateway",
            )
        })?;
    let email = header_str("X-User-Email").unwrap_or_default();
    let username = header_str("X-User-Name").unwrap_or_default();

    Ok(Identity {
        user_id,
        email,
        username,
    })
}

impl FromRequest for Identity {
    type Error = ActixError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(extract(req))
    }
}

/// The same identity, but optional — for endpoints that behave
/// differently when a caller is known (e.g. discovery could eventually
/// personalize results) without requiring a session (DSC-001/DSC-003:
/// browse/search are unauthenticated per SDD §6.1).
pub struct MaybeIdentity(pub Option<Identity>);

impl FromRequest for MaybeIdentity {
    type Error = ActixError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(Ok(Self(extract(req).ok())))
    }
}
