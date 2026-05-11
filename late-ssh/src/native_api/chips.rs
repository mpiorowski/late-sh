use axum::{
    Json, Router,
    extract::State as AxumState,
    routing::get,
};
use late_core::models::chips::UserChips;
use serde::Serialize;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new().route("/api/native/chips", get(get_chips))
}

#[derive(Serialize)]
struct ChipsResponse {
    balance: i64,
    last_stipend_date: Option<String>,
}

async fn get_chips(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<ChipsResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let chips = UserChips::ensure(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;

    Ok(Json(ChipsResponse {
        balance: chips.balance,
        last_stipend_date: chips.last_stipend_date.map(|d| d.to_string()),
    }))
}
