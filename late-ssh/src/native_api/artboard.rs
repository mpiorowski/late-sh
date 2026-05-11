use axum::{
    Json, Router,
    extract::State as AxumState,
    http::StatusCode,
    routing::{get, post},
};
use dartboard_core::CanvasOp;
use rand_core::{OsRng, RngCore};
use serde::Serialize;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/artboard", get(get_artboard))
        .route("/api/native/artboard/ops", post(post_artboard_op))
}

#[derive(Serialize)]
struct ArtboardResponse {
    canvas: serde_json::Value,
}

async fn get_artboard(
    _auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<ArtboardResponse>, ApiError> {
    let canvas = state.dartboard_server.canvas_snapshot();
    let canvas_json =
        serde_json::to_value(&canvas).map_err(|_| ApiError::Db)?;
    Ok(Json(ArtboardResponse { canvas: canvas_json }))
}

async fn post_artboard_op(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(op): Json<CanvasOp>,
) -> Result<StatusCode, ApiError> {
    let user_id_u64 = auth.user_id.as_u64_pair().1;
    let client_op_id: u64 = OsRng.next_u64();
    state.dartboard_server.submit_op_for(user_id_u64, client_op_id, op);
    Ok(StatusCode::ACCEPTED)
}
