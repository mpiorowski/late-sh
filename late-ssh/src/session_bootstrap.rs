use late_core::{
    MutexRecover,
    models::{artboard_ban::ArtboardBan, user::User},
};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;
use crate::app::artboard::svc::ArtboardSnapshotService;
use crate::app::common::theme;
use crate::app::dashboard::state::DashboardRoomJoinReceiver;
use crate::app::state::SessionConfig;
use crate::authz::Permissions;
use crate::session::SessionMessage;
use crate::state::State;

pub struct SessionBootstrapInputs {
    pub user: User,
    pub is_new_user: bool,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
    pub session_token: String,
    pub session_rx: Option<mpsc::Receiver<SessionMessage>>,
    pub activity_feed_rx: Option<broadcast::Receiver<ActivityEvent>>,
    pub room_join_rx: Option<DashboardRoomJoinReceiver>,
}

pub struct ArcadeSessionPreloads {
    pub initial_2048_game: Option<late_core::models::twenty_forty_eight::Game>,
    pub initial_2048_high_score: Option<late_core::models::twenty_forty_eight::HighScore>,
    pub initial_tetris_game: Option<late_core::models::tetris::Game>,
    pub initial_tetris_high_score: Option<late_core::models::tetris::HighScore>,
    pub initial_snake_game: Option<late_core::models::snake::Game>,
    pub initial_snake_high_score: Option<late_core::models::snake::HighScore>,
    pub initial_le_word_daily_word: Option<late_core::models::le_word::DailyWord>,
    pub initial_le_word_game: Option<late_core::models::le_word::Game>,
    pub initial_sudoku_games: Vec<late_core::models::sudoku::Game>,
    pub initial_nonogram_games: Vec<late_core::models::nonogram::Game>,
    pub initial_solitaire_games: Vec<late_core::models::solitaire::Game>,
    pub initial_minesweeper_games: Vec<late_core::models::minesweeper::Game>,
}

pub async fn load_arcade_session_preloads(state: &State, user_id: Uuid) -> ArcadeSessionPreloads {
    let twenty_forty_eight_service = state.twenty_forty_eight_service.clone();
    let tetris_service = state.tetris_service.clone();
    let snake_service = state.snake_service.clone();
    let le_word_service = state.le_word_service.clone();
    let sudoku_service = state.sudoku_service.clone();
    let nonogram_service = state.nonogram_service.clone();
    let solitaire_service = state.solitaire_service.clone();
    let minesweeper_service = state.minesweeper_service.clone();

    let (
        initial_2048_game,
        initial_2048_high_score,
        initial_tetris_game,
        initial_tetris_high_score,
        initial_snake_game,
        initial_snake_high_score,
        initial_le_word_daily_word,
        initial_le_word_game,
        initial_sudoku_games,
        initial_nonogram_games,
        initial_solitaire_games,
        initial_minesweeper_games,
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
            match le_word_service.ensure_daily_word().await {
                Ok(word) => Some(word),
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load Le Word daily word");
                    None
                }
            }
        },
        async {
            let today = le_word_service.today();
            match le_word_service.load_game(user_id, today).await {
                Ok(game) => game,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load Le Word game state");
                    None
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
    );

    ArcadeSessionPreloads {
        initial_2048_game,
        initial_2048_high_score,
        initial_tetris_game,
        initial_tetris_high_score,
        initial_snake_game,
        initial_snake_high_score,
        initial_le_word_daily_word,
        initial_le_word_game,
        initial_sudoku_games,
        initial_nonogram_games,
        initial_solitaire_games,
        initial_minesweeper_games,
    }
}

pub async fn build_session_config(state: &State, inputs: SessionBootstrapInputs) -> SessionConfig {
    let SessionBootstrapInputs {
        user,
        is_new_user,
        cols,
        rows,
        term,
        session_token,
        session_rx,
        activity_feed_rx,
        room_join_rx,
    } = inputs;

    let user_id = user.id;
    let permissions =
        Permissions::new(user.is_admin || state.config.force_admin, user.is_moderator);
    let ArcadeSessionPreloads {
        initial_2048_game,
        initial_2048_high_score,
        initial_tetris_game,
        initial_tetris_high_score,
        initial_snake_game,
        initial_snake_high_score,
        initial_le_word_daily_word,
        initial_le_word_game,
        initial_sudoku_games,
        initial_nonogram_games,
        initial_solitaire_games,
        initial_minesweeper_games,
    } = load_arcade_session_preloads(state, user_id).await;
    let (initial_bonsai_tree, initial_bonsai_care) =
        match state.bonsai_service.ensure_tree_with_care(user_id).await {
            Ok((tree, care)) => (Some(tree), Some(care)),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load/create bonsai tree");
                (None, None)
            }
        };
    let shop_snapshot_rx = state.shop_service.subscribe_snapshot(user_id);
    let shop_snapshot = match state.shop_service.refresh_user(user_id).await {
        Ok(snapshot) => Some(snapshot),
        Err(e) => {
            tracing::warn!(error = ?e, "failed to refresh shop snapshot");
            None
        }
    };
    let initial_bonsai_v2_tree = if shop_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.entitlements.has_dynamic_bonsai())
    {
        match state
            .bonsai_service
            .ensure_v2_tree(user_id, initial_bonsai_tree.as_ref())
            .await
        {
            Ok(tree) => Some(tree),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load/create bonsai v2 tree");
                None
            }
        }
    } else {
        None
    };
    let initial_chip_balance = match state.chip_service.ensure_chips(user_id).await {
        Ok(chips) => chips.balance,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to ensure chip balance");
            0
        }
    };
    let initial_pet = match state.pet_service.ensure_cat(user_id).await {
        Ok(cat) => Some(cat),
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load/create cat companion");
            None
        }
    };
    let quest_snapshot_rx = state.quest_service.subscribe_snapshot(user_id);
    if let Err(e) = state.quest_service.refresh_user(user_id).await {
        tracing::warn!(error = ?e, "failed to refresh quest snapshot");
    }
    let initial_ultimate_cooldowns = match state.ultimate_service.list_cooldowns(user_id).await {
        Ok(cooldowns) => cooldowns,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load ultimate cooldowns");
            Vec::new()
        }
    };
    let artboard_ban = match state.db.get().await {
        Ok(client) => match ArtboardBan::find_active_for_user(&client, user_id).await {
            Ok(ban) => ban,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to check artboard ban status");
                None
            }
        },
        Err(e) => {
            tracing::warn!(error = ?e, "failed to get db client for artboard ban check");
            None
        }
    };
    let initial_announcements = match state.db.get().await {
        Ok(client) => {
            match crate::app::announcements::load_login_announcements(&client, user_id).await {
                Ok(announcements) => announcements,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load login announcements");
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = ?e, "failed to get db client for login announcements");
            None
        }
    };

    SessionConfig {
        cols,
        rows,
        term,
        audio_service: state.audio_service.clone(),
        voice_service: state.voice_service.clone(),
        chat_service: state.chat_service.clone(),
        notification_service: state.notification_service.clone(),
        article_service: state.article_service.clone(),
        feed_service: state.feed_service.clone(),
        showcase_service: state.showcase_service.clone(),
        work_service: state.work_service.clone(),
        profile_service: state.profile_service.clone(),
        twenty_forty_eight_service: state.twenty_forty_eight_service.clone(),
        initial_2048_game,
        initial_2048_high_score,
        tetris_service: state.tetris_service.clone(),
        snake_service: state.snake_service.clone(),
        rubiks_cube_service: state.rubiks_cube_service.clone(),
        initial_tetris_game,
        initial_snake_game,
        initial_tetris_high_score,
        initial_snake_high_score,
        le_word_service: state.le_word_service.clone(),
        initial_le_word_daily_word,
        initial_le_word_game,
        sudoku_service: state.sudoku_service.clone(),
        initial_sudoku_games,
        nonogram_service: state.nonogram_service.clone(),
        initial_nonogram_games,
        solitaire_service: state.solitaire_service.clone(),
        initial_solitaire_games,
        minesweeper_service: state.minesweeper_service.clone(),
        initial_minesweeper_games,
        lateania_service: state.lateania_service.clone(),
        rooms_service: state.rooms_service.clone(),
        room_game_registry: state.room_game_registry.clone(),
        dartboard_server: state.dartboard_server.clone(),
        dartboard_provenance: state.dartboard_provenance.clone(),
        artboard_snapshot_service: ArtboardSnapshotService::new(state.db.clone()),
        username: user.username.clone(),
        bonsai_service: state.bonsai_service.clone(),
        initial_bonsai_tree,
        initial_bonsai_care,
        initial_bonsai_v2_tree,
        pet_service: state.pet_service.clone(),
        initial_pet,
        quest_service: state.quest_service.clone(),
        quest_snapshot_rx,
        shop_service: state.shop_service.clone(),
        shop_snapshot_rx,
        ultimate_service: state.ultimate_service.clone(),
        initial_ultimate_cooldowns,
        nonogram_library: state.nonogram_library.clone(),
        initial_chip_balance,
        web_url: state.config.web_url.clone(),
        rebels_enabled: state.config.rebels_enabled,
        rebels_host: state.config.rebels_host.clone(),
        rebels_port: state.config.rebels_port,
        rebels_secret: state.config.rebels_secret.clone(),
        nethack_enabled: state.config.nethack_enabled,
        nethack_bin: state.config.nethack_bin.clone(),
        nethack_data_dir: state.config.nethack_data_dir.clone(),
        session_token,
        session_registry: Some(state.session_registry.clone()),
        paired_client_registry: Some(state.paired_client_registry.clone()),
        session_rx,
        now_playing_rx: Some(state.now_playing_rx.clone()),
        radio_meta_rx: Some(state.radio_meta_rx.clone()),
        active_users: Some(state.active_users.clone()),
        afk_users: state.afk_users.clone(),
        username_directory: Some(state.username_directory.clone()),
        activity_feed_rx,
        initial_activity: state.activity_history.lock_recover().clone(),
        room_join_rx,
        initial_room_joins: state.room_join_history.lock_recover().clone(),
        initial_announcements,
        user_id,
        permissions,
        artboard_banned: artboard_ban.is_some(),
        artboard_ban_expires_at: artboard_ban.and_then(|ban| ban.expires_at),
        leaderboard_rx: Some(state.leaderboard_service.subscribe()),
        is_new_user,
        initial_theme_id: late_core::models::user::extract_theme_id(&user.settings)
            .unwrap_or_else(|| theme::DEFAULT_ID.to_string()),
        initial_audio_source: late_core::models::user::extract_audio_source(&user.settings),
        initial_icecast_stream: late_core::models::user::extract_icecast_stream(&user.settings),
        initial_radio_station: late_core::models::user::extract_radio_station(&user.settings),
        pinstar_registry: state.pinstar_registry.clone(),
        is_draining: state.is_draining.clone(),
    }
}
