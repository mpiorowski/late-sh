use late_core::{
    MutexRecover,
    models::{artboard_ban::ArtboardBan, user::User},
};
use tokio::sync::{broadcast, mpsc};

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

    let my_vote = match state.vote_service.get_user_vote(user_id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to get user vote");
            None
        }
    };
    let initial_2048_game = match state.twenty_forty_eight_service.load_game(user_id).await {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load 2048 game state");
            None
        }
    };
    let initial_2048_high_score = match state
        .twenty_forty_eight_service
        .load_high_score(user_id)
        .await
    {
        Ok(score) => score,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load 2048 high score");
            None
        }
    };
    let initial_tetris_game = match state.tetris_service.load_game(user_id).await {
        Ok(game) => game,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load tetris game state");
            None
        }
    };
    let initial_tetris_high_score = match state.tetris_service.load_high_score(user_id).await {
        Ok(score) => score,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load tetris high score");
            None
        }
    };
    let initial_snake_game = match state.snake_service.load_game(user_id).await {
        Ok(game) => game,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load snake game state");
            None
        }
    };
    let initial_snake_high_score = match state.snake_service.load_high_score(user_id).await {
        Ok(score) => score,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load snake high score");
            None
        }
    };
    let initial_sudoku_games = match state.sudoku_service.load_games(user_id).await {
        Ok(games) => games,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load sudoku game states");
            Vec::new()
        }
    };
    let initial_nonogram_games = match state.nonogram_service.load_games(user_id).await {
        Ok(games) => games,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load nonogram game states");
            Vec::new()
        }
    };
    let initial_solitaire_games = match state.solitaire_service.load_games(user_id).await {
        Ok(games) => games,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load solitaire game states");
            Vec::new()
        }
    };
    let initial_minesweeper_games = match state.minesweeper_service.load_games(user_id).await {
        Ok(games) => games,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to load minesweeper game states");
            Vec::new()
        }
    };
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

    SessionConfig {
        cols,
        rows,
        term,
        audio_service: state.audio_service.clone(),
        voice_service: state.voice_service.clone(),
        vote_service: state.vote_service.clone(),
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
        initial_tetris_game,
        initial_snake_game,
        initial_tetris_high_score,
        initial_snake_high_score,
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
        session_token,
        session_registry: Some(state.session_registry.clone()),
        paired_client_registry: Some(state.paired_client_registry.clone()),
        session_rx,
        now_playing_rx: Some(state.now_playing_rx.clone()),
        active_users: Some(state.active_users.clone()),
        afk_users: state.afk_users.clone(),
        username_directory: Some(state.username_directory.clone()),
        activity_feed_rx,
        initial_activity: state.activity_history.lock_recover().clone(),
        room_join_rx,
        initial_room_joins: state.room_join_history.lock_recover().clone(),
        user_id,
        permissions,
        artboard_banned: artboard_ban.is_some(),
        artboard_ban_expires_at: artboard_ban.and_then(|ban| ban.expires_at),
        my_vote,
        leaderboard_rx: Some(state.leaderboard_service.subscribe()),
        is_new_user,
        initial_theme_id: late_core::models::user::extract_theme_id(&user.settings)
            .unwrap_or_else(|| theme::DEFAULT_ID.to_string()),
        initial_audio_source: late_core::models::user::extract_audio_source(&user.settings),
        pinstar_registry: state.pinstar_registry.clone(),
        is_draining: state.is_draining.clone(),
    }
}
