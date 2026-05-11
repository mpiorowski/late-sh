use axum::{
    Json, Router,
    extract::{Query, State as AxumState},
    http::StatusCode,
    routing::{get, put},
};
use chrono::Utc;
use late_core::models::{
    leaderboard::{BadgeTier, fetch_leaderboard_data},
    minesweeper, nonogram, solitaire, sudoku, tetris, twenty_forty_eight,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::State;

use super::{ApiError, NativeAuthUser};

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/native/games/leaderboard", get(get_leaderboard))
        // Tetris
        .route("/api/native/games/tetris", get(get_tetris))
        .route("/api/native/games/tetris", put(put_tetris))
        // 2048
        .route("/api/native/games/twenty-forty-eight", get(get_2048))
        .route("/api/native/games/twenty-forty-eight", put(put_2048))
        // Minesweeper
        .route("/api/native/games/minesweeper", get(get_minesweeper))
        .route("/api/native/games/minesweeper", put(put_minesweeper))
        .route("/api/native/games/minesweeper/won-today", get(get_minesweeper_won_today))
        // Sudoku
        .route("/api/native/games/sudoku", get(get_sudoku))
        .route("/api/native/games/sudoku", put(put_sudoku))
        .route("/api/native/games/sudoku/won-today", get(get_sudoku_won_today))
        // Nonogram
        .route("/api/native/games/nonogram", get(get_nonogram))
        .route("/api/native/games/nonogram", put(put_nonogram))
        .route("/api/native/games/nonogram/won-today", get(get_nonogram_won_today))
        // Solitaire
        .route("/api/native/games/solitaire", get(get_solitaire))
        .route("/api/native/games/solitaire", put(put_solitaire))
        .route("/api/native/games/solitaire/won-today", get(get_solitaire_won_today))
}

// ── Leaderboard ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct LeaderboardResponse {
    today_champions: Vec<PlayerEntry>,
    streak_leaders: Vec<PlayerEntry>,
    high_scores: Vec<HighScoreItem>,
    chip_leaders: Vec<ChipItem>,
}

#[derive(Serialize)]
struct PlayerEntry {
    user_id: String,
    username: String,
    count: u32,
    badge: Option<String>,
}

#[derive(Serialize)]
struct HighScoreItem {
    game: String,
    user_id: String,
    username: String,
    score: i32,
}

#[derive(Serialize)]
struct ChipItem {
    user_id: String,
    username: String,
    balance: i64,
}

async fn get_leaderboard(
    _auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<LeaderboardResponse>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let data = fetch_leaderboard_data(&client).await.map_err(|_| ApiError::Db)?;

    let badges = data.badges();

    let today_champions = data
        .today_champions
        .iter()
        .map(|e| PlayerEntry {
            user_id: e.user_id.to_string(),
            username: e.username.clone(),
            count: e.count,
            badge: badges.get(&e.user_id).map(badge_label),
        })
        .collect();

    let streak_leaders = data
        .streak_leaders
        .iter()
        .map(|e| PlayerEntry {
            user_id: e.user_id.to_string(),
            username: e.username.clone(),
            count: e.count,
            badge: badges.get(&e.user_id).map(badge_label),
        })
        .collect();

    let high_scores = data
        .high_scores
        .iter()
        .map(|e| HighScoreItem {
            game: e.game.to_string(),
            user_id: e.user_id.to_string(),
            username: e.username.clone(),
            score: e.score,
        })
        .collect();

    let chip_leaders = data
        .chip_leaders
        .iter()
        .map(|e| ChipItem {
            user_id: e.user_id.to_string(),
            username: e.username.clone(),
            balance: e.balance,
        })
        .collect();

    Ok(Json(LeaderboardResponse { today_champions, streak_leaders, high_scores, chip_leaders }))
}

fn badge_label(tier: &BadgeTier) -> String {
    tier.tier_name().to_string()
}

// ── Tetris ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct TetrisState {
    score: i32,
    lines: i32,
    level: i32,
    board: Value,
    current_kind: String,
    current_rotation: i32,
    current_row: i32,
    current_col: i32,
    next_kind: String,
    is_game_over: bool,
}

#[derive(Deserialize)]
struct PutTetrisBody {
    score: i32,
    lines: i32,
    level: i32,
    board: Value,
    current_kind: String,
    current_rotation: i32,
    current_row: i32,
    current_col: i32,
    next_kind: String,
    is_game_over: bool,
}

async fn get_tetris(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Option<TetrisState>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let games = tetris::Game::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(games.into_iter().next().map(|g| TetrisState {
        score: g.score,
        lines: g.lines,
        level: g.level,
        board: g.board,
        current_kind: g.current_kind,
        current_rotation: g.current_rotation,
        current_row: g.current_row,
        current_col: g.current_col,
        next_kind: g.next_kind,
        is_game_over: g.is_game_over,
    })))
}

async fn put_tetris(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<PutTetrisBody>,
) -> Result<(StatusCode, Json<TetrisState>), ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let g = tetris::Game::upsert(
        &client,
        tetris::GameParams {
            user_id: auth.user_id,
            score: body.score,
            lines: body.lines,
            level: body.level,
            board: body.board,
            current_kind: body.current_kind,
            current_rotation: body.current_rotation,
            current_row: body.current_row,
            current_col: body.current_col,
            next_kind: body.next_kind,
            is_game_over: body.is_game_over,
        },
    )
    .await
    .map_err(|_| ApiError::Db)?;

    if g.is_game_over {
        tetris::HighScore::update_score_if_higher(&client, auth.user_id, g.score)
            .await
            .ok();
    }

    Ok((
        StatusCode::OK,
        Json(TetrisState {
            score: g.score,
            lines: g.lines,
            level: g.level,
            board: g.board,
            current_kind: g.current_kind,
            current_rotation: g.current_rotation,
            current_row: g.current_row,
            current_col: g.current_col,
            next_kind: g.next_kind,
            is_game_over: g.is_game_over,
        }),
    ))
}

// ── 2048 ──────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct TwentyFortyEightState {
    score: i32,
    grid: Value,
    is_game_over: bool,
}

#[derive(Deserialize)]
struct PutTwentyFortyEightBody {
    score: i32,
    grid: Value,
    is_game_over: bool,
}

async fn get_2048(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Option<TwentyFortyEightState>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let games = twenty_forty_eight::Game::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(games.into_iter().next().map(|g| TwentyFortyEightState {
        score: g.score,
        grid: g.grid,
        is_game_over: g.is_game_over,
    })))
}

async fn put_2048(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<PutTwentyFortyEightBody>,
) -> Result<(StatusCode, Json<TwentyFortyEightState>), ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let g = twenty_forty_eight::Game::upsert(
        &client,
        auth.user_id,
        body.score,
        body.grid,
        body.is_game_over,
    )
    .await
    .map_err(|_| ApiError::Db)?;

    if g.is_game_over {
        twenty_forty_eight::HighScore::update_score_if_higher(&client, auth.user_id, g.score)
            .await
            .ok();
    }

    Ok((
        StatusCode::OK,
        Json(TwentyFortyEightState { score: g.score, grid: g.grid, is_game_over: g.is_game_over }),
    ))
}

// ── Minesweeper ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct MinesweeperState {
    mode: String,
    difficulty_key: String,
    puzzle_date: Option<String>,
    puzzle_seed: i64,
    mine_map: Value,
    player_grid: Value,
    lives: i32,
    is_game_over: bool,
    score: i32,
}

#[derive(Deserialize)]
struct PutMinesweeperBody {
    mode: String,
    difficulty_key: String,
    puzzle_date: Option<String>,
    puzzle_seed: i64,
    mine_map: Value,
    player_grid: Value,
    lives: i32,
    is_game_over: bool,
    score: i32,
}

#[derive(Deserialize)]
struct WonTodayParams {
    difficulty_key: String,
}

async fn get_minesweeper(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<MinesweeperState>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let games = minesweeper::Game::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(games.into_iter().map(minesweeper_state).collect()))
}

async fn put_minesweeper(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<PutMinesweeperBody>,
) -> Result<(StatusCode, Json<MinesweeperState>), ApiError> {
    let puzzle_date = body
        .puzzle_date
        .as_deref()
        .map(|s| s.parse::<chrono::NaiveDate>().map_err(|_| ApiError::BadRequest("invalid puzzle_date")))
        .transpose()?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let g = minesweeper::Game::upsert(
        &client,
        minesweeper::GameParams {
            user_id: auth.user_id,
            mode: body.mode,
            difficulty_key: body.difficulty_key,
            puzzle_date,
            puzzle_seed: body.puzzle_seed,
            mine_map: body.mine_map,
            player_grid: body.player_grid,
            lives: body.lives,
            is_game_over: body.is_game_over,
            score: body.score,
        },
    )
    .await
    .map_err(|_| ApiError::Db)?;

    Ok((StatusCode::OK, Json(minesweeper_state(g))))
}

async fn get_minesweeper_won_today(
    auth: NativeAuthUser,
    Query(params): Query<WonTodayParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<WonTodayResponse>, ApiError> {
    let today = Utc::now().date_naive();
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let won = minesweeper::DailyWin::has_won_today(&client, auth.user_id, &params.difficulty_key, today)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(WonTodayResponse { won, date: today.to_string() }))
}

fn minesweeper_state(g: minesweeper::Game) -> MinesweeperState {
    MinesweeperState {
        mode: g.mode,
        difficulty_key: g.difficulty_key,
        puzzle_date: g.puzzle_date.map(|d| d.to_string()),
        puzzle_seed: g.puzzle_seed,
        mine_map: g.mine_map,
        player_grid: g.player_grid,
        lives: g.lives,
        is_game_over: g.is_game_over,
        score: g.score,
    }
}

// ── Sudoku ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SudokuState {
    mode: String,
    difficulty_key: String,
    puzzle_date: Option<String>,
    puzzle_seed: i64,
    grid: Value,
    fixed_mask: Value,
    is_game_over: bool,
    score: i32,
}

#[derive(Deserialize)]
struct PutSudokuBody {
    mode: String,
    difficulty_key: String,
    puzzle_date: Option<String>,
    puzzle_seed: i64,
    grid: Value,
    fixed_mask: Value,
    is_game_over: bool,
    score: i32,
}

async fn get_sudoku(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<SudokuState>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let games = sudoku::Game::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(games.into_iter().map(sudoku_state).collect()))
}

async fn put_sudoku(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<PutSudokuBody>,
) -> Result<(StatusCode, Json<SudokuState>), ApiError> {
    let puzzle_date = body
        .puzzle_date
        .as_deref()
        .map(|s| s.parse::<chrono::NaiveDate>().map_err(|_| ApiError::BadRequest("invalid puzzle_date")))
        .transpose()?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let g = sudoku::Game::upsert(
        &client,
        sudoku::GameParams {
            user_id: auth.user_id,
            mode: body.mode,
            difficulty_key: body.difficulty_key,
            puzzle_date,
            puzzle_seed: body.puzzle_seed,
            grid: body.grid,
            fixed_mask: body.fixed_mask,
            is_game_over: body.is_game_over,
            score: body.score,
        },
    )
    .await
    .map_err(|_| ApiError::Db)?;

    Ok((StatusCode::OK, Json(sudoku_state(g))))
}

async fn get_sudoku_won_today(
    auth: NativeAuthUser,
    Query(params): Query<WonTodayParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<WonTodayResponse>, ApiError> {
    let today = Utc::now().date_naive();
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let won = sudoku::DailyWin::has_won_today(&client, auth.user_id, &params.difficulty_key, today)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(WonTodayResponse { won, date: today.to_string() }))
}

fn sudoku_state(g: sudoku::Game) -> SudokuState {
    SudokuState {
        mode: g.mode,
        difficulty_key: g.difficulty_key,
        puzzle_date: g.puzzle_date.map(|d| d.to_string()),
        puzzle_seed: g.puzzle_seed,
        grid: g.grid,
        fixed_mask: g.fixed_mask,
        is_game_over: g.is_game_over,
        score: g.score,
    }
}

// ── Nonogram ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct NonogramState {
    mode: String,
    size_key: String,
    puzzle_date: Option<String>,
    puzzle_id: String,
    player_grid: Value,
    is_game_over: bool,
    score: i32,
}

#[derive(Deserialize)]
struct PutNonogramBody {
    mode: String,
    size_key: String,
    puzzle_date: Option<String>,
    puzzle_id: String,
    player_grid: Value,
    is_game_over: bool,
    score: i32,
}

#[derive(Deserialize)]
struct NonogramWonTodayParams {
    size_key: String,
}

async fn get_nonogram(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<NonogramState>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let games = nonogram::Game::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(games.into_iter().map(nonogram_state).collect()))
}

async fn put_nonogram(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<PutNonogramBody>,
) -> Result<(StatusCode, Json<NonogramState>), ApiError> {
    let puzzle_date = body
        .puzzle_date
        .as_deref()
        .map(|s| s.parse::<chrono::NaiveDate>().map_err(|_| ApiError::BadRequest("invalid puzzle_date")))
        .transpose()?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let g = nonogram::Game::upsert(
        &client,
        nonogram::GameParams {
            user_id: auth.user_id,
            mode: body.mode,
            size_key: body.size_key,
            puzzle_date,
            puzzle_id: body.puzzle_id,
            player_grid: body.player_grid,
            is_game_over: body.is_game_over,
            score: body.score,
        },
    )
    .await
    .map_err(|_| ApiError::Db)?;

    Ok((StatusCode::OK, Json(nonogram_state(g))))
}

async fn get_nonogram_won_today(
    auth: NativeAuthUser,
    Query(params): Query<NonogramWonTodayParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<WonTodayResponse>, ApiError> {
    let today = Utc::now().date_naive();
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let won = nonogram::DailyWin::has_won_today(&client, auth.user_id, &params.size_key, today)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(WonTodayResponse { won, date: today.to_string() }))
}

fn nonogram_state(g: nonogram::Game) -> NonogramState {
    NonogramState {
        mode: g.mode,
        size_key: g.size_key,
        puzzle_date: g.puzzle_date.map(|d| d.to_string()),
        puzzle_id: g.puzzle_id,
        player_grid: g.player_grid,
        is_game_over: g.is_game_over,
        score: g.score,
    }
}

// ── Solitaire ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SolitaireState {
    mode: String,
    difficulty_key: String,
    puzzle_date: Option<String>,
    puzzle_seed: i64,
    stock: Value,
    waste: Value,
    foundations: Value,
    tableau: Value,
    is_game_over: bool,
    score: i32,
}

#[derive(Deserialize)]
struct PutSolitaireBody {
    mode: String,
    difficulty_key: String,
    puzzle_date: Option<String>,
    puzzle_seed: i64,
    stock: Value,
    waste: Value,
    foundations: Value,
    tableau: Value,
    is_game_over: bool,
    score: i32,
}

async fn get_solitaire(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
) -> Result<Json<Vec<SolitaireState>>, ApiError> {
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let games = solitaire::Game::list_by_user_id(&client, auth.user_id)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(games.into_iter().map(solitaire_state).collect()))
}

async fn put_solitaire(
    auth: NativeAuthUser,
    AxumState(state): AxumState<State>,
    Json(body): Json<PutSolitaireBody>,
) -> Result<(StatusCode, Json<SolitaireState>), ApiError> {
    let puzzle_date = body
        .puzzle_date
        .as_deref()
        .map(|s| s.parse::<chrono::NaiveDate>().map_err(|_| ApiError::BadRequest("invalid puzzle_date")))
        .transpose()?;

    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let g = solitaire::Game::upsert(
        &client,
        solitaire::GameParams {
            user_id: auth.user_id,
            mode: body.mode,
            difficulty_key: body.difficulty_key,
            puzzle_date,
            puzzle_seed: body.puzzle_seed,
            stock: body.stock,
            waste: body.waste,
            foundations: body.foundations,
            tableau: body.tableau,
            is_game_over: body.is_game_over,
            score: body.score,
        },
    )
    .await
    .map_err(|_| ApiError::Db)?;

    Ok((StatusCode::OK, Json(solitaire_state(g))))
}

async fn get_solitaire_won_today(
    auth: NativeAuthUser,
    Query(params): Query<WonTodayParams>,
    AxumState(state): AxumState<State>,
) -> Result<Json<WonTodayResponse>, ApiError> {
    let today = Utc::now().date_naive();
    let client = state.db.get().await.map_err(|_| ApiError::Db)?;
    let won = solitaire::DailyWin::has_won_today(&client, auth.user_id, &params.difficulty_key, today)
        .await
        .map_err(|_| ApiError::Db)?;
    Ok(Json(WonTodayResponse { won, date: today.to_string() }))
}

fn solitaire_state(g: solitaire::Game) -> SolitaireState {
    SolitaireState {
        mode: g.mode,
        difficulty_key: g.difficulty_key,
        puzzle_date: g.puzzle_date.map(|d| d.to_string()),
        puzzle_seed: g.puzzle_seed,
        stock: g.stock,
        waste: g.waste,
        foundations: g.foundations,
        tableau: g.tableau,
        is_game_over: g.is_game_over,
        score: g.score,
    }
}

// ── Shared ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WonTodayResponse {
    won: bool,
    date: String,
}
