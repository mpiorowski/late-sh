use axum::{
    Json, Router,
    extract::{State as AxumState},
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::app::vote::svc::{Genre, VoteSnapshot};
use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/now-playing", get(get_now_playing))
        .route("/api/native/status", get(get_native_status))
        .route("/api/native/vote", post(post_vote))
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct NowPlayingResponse {
    pub track: String,
    pub artist: String,
    pub album: String,
    pub progress_sec: u64,
    pub duration_sec: u64,
    pub volume_pct: u32,
}

#[derive(Serialize, Clone)]
pub struct VotesResponse {
    pub lofi: i64,
    pub ambient: i64,
    pub classic: i64,
    pub jazz: i64,
    pub next_vote_at: String,
}

#[derive(Serialize)]
struct NativeStatusResponse {
    connected: bool,
    online_users: usize,
    now_playing: NowPlayingResponse,
    votes: VotesResponse,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn get_now_playing(
    _auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<NowPlayingResponse> {
    Json(build_now_playing_response(&state))
}

async fn get_native_status(
    _auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Json<NativeStatusResponse> {
    let online_users = state.active_users.lock().unwrap_or_else(|e| e.into_inner()).len();
    Json(NativeStatusResponse {
        connected: true,
        online_users,
        now_playing: build_now_playing_response(&state),
        votes: build_votes_response(&state),
    })
}

#[derive(Deserialize)]
struct VoteBody {
    genre: String,
}

async fn post_vote(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<VoteBody>,
) -> Result<Json<VotesResponse>, ApiError> {
    let genre = Genre::try_from(body.genre.as_str())
        .map_err(|_| ApiError::BadRequest("unknown genre"))?;
    state.vote_service.cast_vote_task(auth.user_id, genre);
    Ok(Json(build_votes_response(&state)))
}

// ── Builder helpers (used by ws.rs) ───────────────────────────────────────────

pub(crate) fn build_now_playing_response(state: &State) -> NowPlayingResponse {
    build_now_playing_from_value(&state.now_playing_rx.borrow().clone())
}

pub(crate) fn build_now_playing_from_value(
    np: &Option<late_core::api_types::NowPlaying>,
) -> NowPlayingResponse {
    match np {
        Some(np) => {
            let elapsed = np.started_at.elapsed().as_secs();
            let duration = np.track.duration_seconds.unwrap_or(1);
            NowPlayingResponse {
                track: np.track.title.clone(),
                artist: np.track.artist.clone().unwrap_or_default(),
                album: String::new(),
                progress_sec: elapsed,
                duration_sec: duration,
                volume_pct: 0,
            }
        }
        None => NowPlayingResponse {
            track: String::new(),
            artist: String::new(),
            album: String::new(),
            progress_sec: 0,
            duration_sec: 1,
            volume_pct: 0,
        },
    }
}

pub(crate) fn build_votes_response(state: &State) -> VotesResponse {
    build_votes_response_from_snapshot(&state.vote_service.subscribe_state().borrow().clone())
}

pub(crate) fn build_votes_response_from_snapshot(snap: &VoteSnapshot) -> VotesResponse {
    let next_vote_at =
        (Utc::now() + chrono::Duration::from_std(snap.next_switch_in).unwrap_or_default())
            .to_rfc3339();
    VotesResponse {
        lofi: snap.counts.lofi,
        ambient: snap.counts.ambient,
        classic: snap.counts.classic,
        jazz: snap.counts.jazz,
        next_vote_at,
    }
}
