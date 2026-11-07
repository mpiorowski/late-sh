use chrono::{DateTime, Utc};
use late_core::models::user::User;
use late_core::models::user_ssh_key::{
    UserSshKey, extract_key_artboard_disclaimer, extract_key_home_layout, extract_key_home_layouts,
};
use uuid::Uuid;

use crate::state::State;

pub struct ArcadeSessionPreloads {
    pub initial_2048_game: Option<late_core::models::twenty_forty_eight::Game>,
    pub initial_2048_high_score: Option<late_core::models::twenty_forty_eight::HighScore>,
    pub initial_tetris_game: Option<late_core::models::tetris::Game>,
    pub initial_tetris_high_score: Option<late_core::models::tetris::HighScore>,
    pub initial_snake_game: Option<late_core::models::snake::Game>,
    pub initial_snake_high_score: Option<late_core::models::snake::HighScore>,
    pub initial_traffic_track_scores: Vec<late_core::models::traffic::TrackScore>,
    pub initial_traffic_high_score: Option<late_core::models::traffic::HighScore>,
    pub initial_le_word_today: Option<crate::app::arcade::le_word::svc::Today>,
    pub initial_rubiks_cube_game: Option<late_core::models::rubiks_cube::Game>,
    pub initial_sliding_puzzle_games: Vec<late_core::models::sliding_puzzle::Game>,
    pub initial_sudoku_games: Vec<late_core::models::sudoku::Game>,
    pub initial_nonogram_games: Vec<late_core::models::nonogram::Game>,
    pub initial_solitaire_games: Vec<late_core::models::solitaire::Game>,
    pub initial_minesweeper_games: Vec<late_core::models::minesweeper::Game>,
    pub initial_netwalk_games: Vec<late_core::models::netwalk::Game>,
    pub initial_chess_puzzle_today: Option<crate::app::arcade::chess_puzzle::svc::Today>,
    pub initial_chess_puzzle_games: Vec<late_core::models::chess_puzzle::Game>,
    pub initial_word_wheel_today: Option<crate::app::arcade::word_wheel::svc::Today>,
    pub initial_quartets_today: Option<crate::app::arcade::quartets::svc::Today>,
}

pub async fn load_arcade_session_preloads(state: &State, user_id: Uuid) -> ArcadeSessionPreloads {
    let twenty_forty_eight_service = state.twenty_forty_eight_service.clone();
    let tetris_service = state.tetris_service.clone();
    let snake_service = state.snake_service.clone();
    let traffic_service = state.traffic_service.clone();
    let le_word_service = state.le_word_service.clone();
    let rubiks_cube_service = state.rubiks_cube_service.clone();
    let sliding_puzzle_service = state.sliding_puzzle_service.clone();
    let sudoku_service = state.sudoku_service.clone();
    let nonogram_service = state.nonogram_service.clone();
    let solitaire_service = state.solitaire_service.clone();
    let minesweeper_service = state.minesweeper_service.clone();
    let netwalk_service = state.netwalk_service.clone();
    let chess_puzzle_service = state.chess_puzzle_service.clone();
    let word_wheel_service = state.word_wheel_service.clone();
    let quartets_service = state.quartets_service.clone();

    let (
        initial_2048_game,
        initial_2048_high_score,
        initial_tetris_game,
        initial_tetris_high_score,
        initial_snake_game,
        initial_snake_high_score,
        initial_traffic_track_scores,
        initial_traffic_high_score,
        initial_le_word_today,
        initial_rubiks_cube_game,
        initial_sliding_puzzle_games,
        initial_sudoku_games,
        initial_nonogram_games,
        initial_solitaire_games,
        initial_minesweeper_games,
        initial_netwalk_games,
        initial_chess_puzzle_today,
        initial_chess_puzzle_games,
        initial_word_wheel_today,
        initial_quartets_today,
    ) = tokio::join!(
        async {
            match twenty_forty_eight_service.load_game(user_id).await {
                Ok(game) => game,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load 2048 game state");
                    None
                }
            }
        },
        async {
            match twenty_forty_eight_service.load_high_score(user_id).await {
                Ok(score) => score,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load 2048 high score");
                    None
                }
            }
        },
        async {
            match tetris_service.load_game(user_id).await {
                Ok(game) => game,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load Lateris game state");
                    None
                }
            }
        },
        async {
            match tetris_service.load_high_score(user_id).await {
                Ok(score) => score,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load Lateris high score");
                    None
                }
            }
        },
        async {
            match snake_service.load_game(user_id).await {
                Ok(game) => game,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load snake game state");
                    None
                }
            }
        },
        async {
            match snake_service.load_high_score(user_id).await {
                Ok(score) => score,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load snake high score");
                    None
                }
            }
        },
        async {
            match traffic_service.load_track_scores(user_id).await {
                Ok(scores) => scores,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load traffic track scores");
                    Vec::new()
                }
            }
        },
        async {
            match traffic_service.load_high_score(user_id).await {
                Ok(score) => score,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load traffic high score");
                    None
                }
            }
        },
        le_word_service.load_today(user_id),
        async {
            match rubiks_cube_service.load_game(user_id).await {
                Ok(game) => game,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load Rubik's Cube game state");
                    None
                }
            }
        },
        async {
            match sliding_puzzle_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load Sliding Puzzle game states");
                    Vec::new()
                }
            }
        },
        async {
            match sudoku_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load sudoku game states");
                    Vec::new()
                }
            }
        },
        async {
            match nonogram_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load nonogram game states");
                    Vec::new()
                }
            }
        },
        async {
            match solitaire_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load solitaire game states");
                    Vec::new()
                }
            }
        },
        async {
            match minesweeper_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load minesweeper game states");
                    Vec::new()
                }
            }
        },
        async {
            match netwalk_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load netwalk game states");
                    Vec::new()
                }
            }
        },
        // Today's puzzles with the saved progress on them (`daily_load.rs`
        // logs and counts each).
        chess_puzzle_service.load_today(user_id),
        // The saved rows again on their own, so the personal puzzles restore
        // whether or not the day's claim did.
        async {
            match chess_puzzle_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load chess puzzle game states");
                    Vec::new()
                }
            }
        },
        word_wheel_service.load_today(user_id),
        quartets_service.load_today(user_id),
    );

    ArcadeSessionPreloads {
        initial_2048_game,
        initial_2048_high_score,
        initial_tetris_game,
        initial_tetris_high_score,
        initial_snake_game,
        initial_snake_high_score,
        initial_traffic_track_scores,
        initial_traffic_high_score,
        initial_le_word_today,
        initial_rubiks_cube_game,
        initial_sliding_puzzle_games,
        initial_sudoku_games,
        initial_nonogram_games,
        initial_solitaire_games,
        initial_minesweeper_games,
        initial_netwalk_games,
        initial_chess_puzzle_today,
        initial_chess_puzzle_games,
        initial_word_wheel_today,
        initial_quartets_today,
    }
}

/// What the SSH key a session authenticated with remembers about its device.
/// Every field is `None` for a keyless session or a key with nothing stored,
/// and every consumer treats `None` as "no device fact": the rails and the
/// Artboard disclaimer follow the account default and a bare `/summary` opens
/// its default window. A failed read is logged and swallowed rather than
/// failing the connection; it reads as `None` too, except for the disclaimer,
/// which is a content gate and fails closed (`Some(true)`): a device that
/// turned the gate on must not lose it to a dropped query when the account
/// seed is off.
#[derive(Clone, Debug, Default)]
pub struct DeviceState {
    /// This device's single Home tile layout, stored before Home had named
    /// layouts (`app/zen`), `None` to follow the account's stored layout.
    /// Read only as the seed for `home_layouts`.
    pub home_layout: Option<serde_json::Value>,
    /// This device's named Home layouts (`app/zen/presets`), `None` until
    /// it saved or switched one; the one above then seeds the set.
    pub home_layouts: Option<serde_json::Value>,
    /// This device's Artboard content disclaimer, `None` to follow the account.
    /// `Some(true)` when the key could not be read.
    pub artboard_disclaimer: Option<bool>,
    /// When the last session on this device went quiet before it ended.
    /// Taken, not read: the row is cleared by the read, so this session is
    /// the only one that will ever be handed this particular leave.
    pub left_at: Option<DateTime<Utc>>,
}

pub async fn load_device_state(
    state: &State,
    user_id: Uuid,
    key_fingerprint: Option<&str>,
) -> DeviceState {
    let Some(fingerprint) = key_fingerprint else {
        return DeviceState::default();
    };
    let client = match state.db.get().await {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to get db client for device state");
            return DeviceState {
                artboard_disclaimer: Some(true),
                ..DeviceState::default()
            };
        }
    };
    let settings = match UserSshKey::find_by_fingerprint(&client, user_id, fingerprint).await {
        Ok(Some(key)) => DeviceState {
            artboard_disclaimer: extract_key_artboard_disclaimer(&key.settings),
            home_layout: extract_key_home_layout(&key.settings),
            home_layouts: extract_key_home_layouts(&key.settings),
            left_at: None,
        },
        Ok(None) => DeviceState::default(),
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load device settings");
            DeviceState {
                artboard_disclaimer: Some(true),
                ..DeviceState::default()
            }
        }
    };
    let left_at = match UserSshKey::take_left_at(&client, user_id, fingerprint).await {
        Ok(left_at) => left_at,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to take device left_at");
            None
        }
    };
    DeviceState {
        left_at,
        ..settings
    }
}

impl DeviceState {
    /// Whether this session shows the Artboard content disclaimer: this
    /// device's choice when its key has one, else the account's.
    pub fn shows_artboard_disclaimer(&self, user: &User) -> bool {
        self.artboard_disclaimer
            .unwrap_or_else(|| late_core::models::user::extract_artboard_disclaimer(&user.settings))
    }
}
