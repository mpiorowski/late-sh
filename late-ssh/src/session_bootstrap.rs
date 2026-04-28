//! Shared per-session bootstrap. Loads user-state from the DB and
//! assembles a `SessionConfig` for `App::new`.
//!
//! Used by both the russh `pty_request` callback and the `/tunnel` WS
//! handler so each transport spends its own code only on its own concerns
//! (authentication, transport-shape inputs) and shares the heavy
//! per-session state-loading.
//!
//! Caller-side concerns intentionally NOT done here, per the seam
//! discipline in `PERSISTENT-CONNECTION-GATEWAY.md` §6 and the Phase 2b
//! advisor notes:
//!   - authentication (pubkey on russh / pre-shared secret + handshake
//!     headers on /tunnel)
//!   - per-IP rate limiting
//!   - per-IP / global connection-count semaphores
//!   - ban / unknown-user rejection (caller closes with the appropriate
//!     close code / russh-disconnect)
//!
//! The helper trusts `inputs.user` to be a fully authenticated, ready-to-
//! play user and trusts the caller to have passed the appropriate
//! transport-shaped session_token / session_rx / activity_feed_rx.

use late_core::models::user::User;
use tokio::sync::{broadcast, mpsc};

use crate::app::artboard::svc::ArtboardSnapshotService;
use crate::app::state::SessionConfig;
use crate::session::SessionMessage;
use crate::ssh::late_ssh_theme_id;
use crate::state::{ActivityEvent, State};

/// Caller-supplied inputs that differ between transports. Everything
/// else needed to build a `SessionConfig` is loaded from the state +
/// the user inside `build_session_config`.
pub struct SessionBootstrapInputs {
    pub user: User,
    pub is_new_user: bool,
    pub cols: u16,
    pub rows: u16,
    pub session_token: String,
    /// Browser-pair channel. `Some` when the transport opened a paired
    /// CLI/web session for this user; `None` for transports that don't
    /// support pairing (today: bastion `/tunnel`).
    pub session_rx: Option<mpsc::Receiver<SessionMessage>>,
    /// Activity-feed subscription. Both transports want this; the
    /// caller subscribes so it can drop the receiver cleanly on
    /// connection teardown without the helper holding state.
    pub activity_feed_rx: Option<broadcast::Receiver<ActivityEvent>>,
}

/// Load user-state from the DB and assemble a fully-formed
/// `SessionConfig`. Best-effort on each load: on failure, log + use the
/// safe default the prior inline code used (`None`, `Vec::new()`,
/// `0`, etc.) — no network blip should keep a user from getting into
/// the TUI.
pub async fn build_session_config(state: &State, inputs: SessionBootstrapInputs) -> SessionConfig {
    let SessionBootstrapInputs {
        user,
        is_new_user,
        cols,
        rows,
        session_token,
        session_rx,
        activity_feed_rx,
    } = inputs;

    let user_id = user.id;

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
    let initial_sudoku_games = match state.sudoku_service.load_games(user_id).await {
        Ok(g) => g,
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

    // Grant daily chip stipend on login.
    let initial_chip_balance = match state.chip_service.ensure_chips(user_id).await {
        Ok(chips) => chips.balance,
        Err(e) => {
            tracing::warn!(error = ?e, "failed to grant daily chip stipend");
            0
        }
    };

    SessionConfig {
        cols,
        rows,

        vote_service: state.vote_service.clone(),
        chat_service: state.chat_service.clone(),
        notification_service: state.notification_service.clone(),
        article_service: state.article_service.clone(),
        showcase_service: state.showcase_service.clone(),
        profile_service: state.profile_service.clone(),
        twenty_forty_eight_service: state.twenty_forty_eight_service.clone(),
        initial_2048_game,
        initial_2048_high_score,
        tetris_service: state.tetris_service.clone(),
        initial_tetris_game,
        initial_tetris_high_score,
        sudoku_service: state.sudoku_service.clone(),
        initial_sudoku_games,
        nonogram_service: state.nonogram_service.clone(),
        initial_nonogram_games,
        solitaire_service: state.solitaire_service.clone(),
        initial_solitaire_games,
        minesweeper_service: state.minesweeper_service.clone(),
        initial_minesweeper_games,
        rooms_service: state.rooms_service.clone(),
        blackjack_table_manager: state.blackjack_table_manager.clone(),
        blackjack_service: state.blackjack_service.clone(),
        dartboard_server: state.dartboard_server.clone(),
        dartboard_provenance: state.dartboard_provenance.clone(),
        artboard_snapshot_service: ArtboardSnapshotService::new(state.db.clone()),
        username: user.username.clone(),
        bonsai_service: state.bonsai_service.clone(),
        initial_bonsai_tree,
        initial_bonsai_care,
        nonogram_library: state.nonogram_library.clone(),
        initial_chip_balance,
        leaderboard_rx: Some(state.leaderboard_service.subscribe()),

        web_url: state.config.web_url.clone(),
        session_token,
        session_registry: Some(state.session_registry.clone()),
        paired_client_registry: Some(state.paired_client_registry.clone()),
        web_chat_registry: Some(state.web_chat_registry.clone()),
        session_rx,
        now_playing_rx: Some(state.now_playing_rx.clone()),
        active_users: Some(state.active_users.clone()),
        activity_feed_rx,
        user_id,
        is_admin: user.is_admin || state.config.force_admin,
        is_mod: user.is_mod,

        my_vote,
        is_new_user,

        initial_theme_id: late_ssh_theme_id(&user.settings),

        is_draining: state.is_draining.clone(),
    }
}
