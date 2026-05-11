use axum::{
    Json, Router,
    extract::State as AxumState,
    routing::{get, post},
};
use late_core::models::bonsai::Tree;
use rand_core::{OsRng, RngCore};
use serde::Serialize;

use crate::app::bonsai::{state::stage_for, ui::tree_ascii};
use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/bonsai", get(get_bonsai))
        .route("/api/native/bonsai/water", post(post_bonsai_water))
}

#[derive(Serialize)]
struct BonsaiResponse {
    growth_points: i32,
    is_alive: bool,
    last_watered: Option<String>,
    art: Vec<String>,
}

async fn get_bonsai(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<BonsaiResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let tree = Tree::ensure(&client, auth.user_id, rand_seed())
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(build_response(tree)))
}

async fn post_bonsai_water(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<BonsaiResponse>, ApiError> {
    state.bonsai_service.water_task(auth.user_id, false);

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let tree = Tree::ensure(&client, auth.user_id, rand_seed())
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(build_response(tree)))
}

fn build_response(tree: Tree) -> BonsaiResponse {
    let art = tree_ascii(stage_for(tree.is_alive, tree.growth_points), tree.seed, false);
    BonsaiResponse {
        growth_points: tree.growth_points,
        is_alive: tree.is_alive,
        last_watered: tree.last_watered.map(|d| d.to_string()),
        art,
    }
}

fn rand_seed() -> i64 {
    OsRng.next_u64() as i64
}
