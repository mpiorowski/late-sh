use axum::{
    Json, Router,
    extract::{ConnectInfo, State as AxumState},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
};
use chrono::{Duration, Utc};
use late_core::models::{native_token::NativeToken, user::User};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use super::{ApiError, NativeAuthUser};
use crate::state::State;

const TOKEN_DAYS: i64 = 30;

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/challenge", get(get_challenge))
        .route("/api/native/token", post(post_token))
        .route("/api/native/logout", delete(delete_token))
        .route("/api/native/ws-ticket", get(get_ws_ticket))
}

// ── Challenge ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChallengeResponse {
    nonce: String,
    expires_in: u32,
}

async fn get_challenge(
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxumState(state): AxumState<State>,
) -> impl IntoResponse {
    let client_ip = crate::api::effective_client_ip(&headers, peer_addr, &state);
    if !state.native_challenge_limiter.allow(client_ip) {
        return ApiError::TooManyRequests.into_response();
    }
    let nonce = crate::session::new_session_token();
    state.native_challenges.issue(nonce.clone());
    Json(ChallengeResponse { nonce, expires_in: 60 }).into_response()
}

// ── Token ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenRequest {
    /// SHA-256 fingerprint in `SHA256:xxxx` format (e.g. from `ssh-keygen -lf`).
    public_key_fingerprint: String,
    /// OpenSSH public key string, e.g. `"ssh-ed25519 AAAA... comment"`.
    public_key: String,
    /// Nonce from `GET /api/native/challenge`.
    nonce: String,
    /// Full PEM text of the SSH signature produced by `ssh-keygen -Y sign -n late.sh`.
    signature_pem: String,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
    expires_at: String,
}

async fn post_token(
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxumState(state): AxumState<State>,
    Json(body): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let client_ip = crate::api::effective_client_ip(&headers, peer_addr, &state);
    if !state.native_token_limiter.allow(client_ip) {
        return Err(ApiError::TooManyRequests);
    }

    if !state.native_challenges.consume(&body.nonce) {
        return Err(ApiError::Unauthorized("nonce invalid or expired"));
    }

    verify_ssh_sig(&body.public_key, &body.public_key_fingerprint, &body.nonce, &body.signature_pem)
        .map_err(|_| ApiError::Unauthorized("signature verification failed"))?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;

    let user = User::find_by_fingerprint(&client, &body.public_key_fingerprint)
        .await
        .map_err(|_| ApiError::Db)?
        .ok_or(ApiError::Unauthorized("no user with that fingerprint"))?;

    let raw_token = crate::session::new_session_token();
    let expires_at = Utc::now() + Duration::days(TOKEN_DAYS);
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let created_ip = client_ip.to_string();

    NativeToken::create(
        &client,
        &raw_token,
        user.id,
        expires_at,
        user_agent.as_deref(),
        Some(&created_ip),
    )
    .await
    .map_err(|_| ApiError::Db)?;

    Ok(Json(TokenResponse { token: raw_token, expires_at: expires_at.to_rfc3339() }))
}

// ── Logout ────────────────────────────────────────────────────────────────────

async fn delete_token(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<StatusCode, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    NativeToken::delete(&client, &auth.raw_token).await.map_err(|_| ApiError::Db)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── WS ticket ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WsTicketResponse {
    ticket: String,
    expires_in: u32,
}

async fn get_ws_ticket(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<WsTicketResponse> {
    let ticket = state.native_ws_tickets.mint(auth.user_id, auth.username);
    Json(WsTicketResponse { ticket, expires_in: 30 })
}

// ── SSH sig verification ──────────────────────────────────────────────────────

/// Verify an SSH signature produced by `ssh-keygen -Y sign -n late.sh`.
///
/// Checks:
///  1. Public key parses and its SHA-256 fingerprint matches `expected_fingerprint`.
///  2. The PEM signature is valid over `nonce` bytes with namespace `"late.sh"`.
fn verify_ssh_sig(
    public_key_openssh: &str,
    expected_fingerprint: &str,
    nonce: &str,
    signature_pem: &str,
) -> anyhow::Result<()> {
    use russh::keys::{
        PublicKey,
        ssh_key::{HashAlg, SshSig},
    };

    let pk = PublicKey::from_openssh(public_key_openssh)
        .map_err(|e| anyhow::anyhow!("invalid public key: {e}"))?;

    let computed_fp = pk.fingerprint(HashAlg::Sha256).to_string();
    if computed_fp != expected_fingerprint {
        anyhow::bail!("fingerprint mismatch: expected {expected_fingerprint}, got {computed_fp}");
    }

    let sig = SshSig::from_pem(signature_pem)
        .map_err(|e| anyhow::anyhow!("invalid SSH signature: {e}"))?;

    pk.verify("late.sh", nonce.as_bytes(), &sig)
        .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))?;

    Ok(())
}
