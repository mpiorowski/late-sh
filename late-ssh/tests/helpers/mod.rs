#![allow(dead_code)]

use late_core::{
    api_types::NowPlaying,
    db::Db,
    rate_limit::IpRateLimiter,
    test_utils::{TestDb, test_db},
};
use late_ssh::app::ai::svc::AiService;
use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::bonsai::svc::BonsaiService;
use late_ssh::app::chat::news::svc::ArticleService;
use late_ssh::app::chat::notifications::svc::NotificationService;
use late_ssh::app::chat::svc::ChatService;
use late_ssh::app::games::chips::svc::ChipService;
use late_ssh::app::games::leaderboard::svc::LeaderboardService;
use late_ssh::app::games::minesweeper::svc::MinesweeperService;
use late_ssh::app::games::nonogram::state::Library as NonogramLibrary;
use late_ssh::app::games::nonogram::svc::NonogramService;
use late_ssh::app::games::solitaire::svc::SolitaireService;
use late_ssh::app::games::sudoku::svc::SudokuService;
use late_ssh::app::games::tetris::svc::TetrisService;
use late_ssh::app::games::twenty_forty_eight::svc::TwentyFortyEightService;
use late_ssh::app::profile::svc::ProfileService;
use late_ssh::app::rooms::blackjack::manager::BlackjackTableManager;
use late_ssh::app::rooms::blackjack::svc::BlackjackService;
use late_ssh::app::rooms::svc::RoomsService;
use late_ssh::app::state::{App, SessionConfig};
use late_ssh::app::vote::svc::VoteService;
use late_ssh::config::{AiConfig, Config};
use late_ssh::session::{PairControlMessage, PairedClientRegistry, SessionRegistry};
use late_ssh::state::ActivityEvent;
use late_ssh::state::State;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::{Semaphore, broadcast, watch};
use tokio::time::{Duration, Instant, sleep};
use uuid::Uuid;

pub async fn new_test_db() -> TestDb {
    test_db().await
}

fn test_dartboard_server() -> dartboard_local::ServerHandle {
    late_ssh::dartboard::spawn_server()
}

fn test_dartboard_provenance() -> late_ssh::app::artboard::provenance::SharedArtboardProvenance {
    ArtboardProvenance::default().shared()
}

pub fn test_config(db_config: late_core::db::DbConfig) -> Config {
    Config {
        ssh_port: 0,
        api_port: 0,
        icecast_url: "http://localhost:8000".to_string(),
        web_url: "http://localhost:3000".to_string(),
        open_access: true,
        force_admin: false,
        db: db_config,
        max_conns_global: 100,
        max_conns_per_ip: 3,
        ssh_idle_timeout: 60,
        server_key_path: std::env::temp_dir().join(format!("late-ssh-test-key-{}", Uuid::now_v7())),
        allowed_origins: vec!["http://localhost:3000".to_string()],
        liquidsoap_addr: "127.0.0.1:0".to_string(),
        frame_drop_log_every: 100,
        vote_switch_interval_secs: 60 * 60,
        ssh_max_attempts_per_ip: 30,
        ssh_rate_limit_window_secs: 60,
        ssh_proxy_protocol: false,
        ssh_proxy_trusted_cidrs: vec![],
        ws_pair_max_attempts_per_ip: 30,
        ws_pair_rate_limit_window_secs: 60,
        ai: AiConfig {
            enabled: false,
            api_key: None,
            model: "gemini-3.1-pro-preview".to_string(),
        },
    }
}

pub fn test_app_state(db: Db, config: Config) -> State {
    let active_users = Arc::new(Mutex::new(HashMap::new()));
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(64);
    let vote_service = VoteService::new(
        db.clone(),
        "127.0.0.1:0".to_string(),
        Duration::from_secs(config.vote_switch_interval_secs),
        active_users.clone(),
        activity_tx.clone(),
    );
    let notification_service = NotificationService::new(db.clone());
    let chat_service = ChatService::new(db.clone(), notification_service.clone());
    let ai_service = AiService::new(false, None, "gemini-3.1-pro-preview".to_string());
    let article_service = ArticleService::new(db.clone(), ai_service.clone(), chat_service.clone());
    let showcase_service = late_ssh::app::chat::showcase::svc::ShowcaseService::new(db.clone());
    let ssh_attempt_limiter = IpRateLimiter::new(
        config.ssh_max_attempts_per_ip,
        config.ssh_rate_limit_window_secs,
    );
    let ws_pair_limiter = IpRateLimiter::new(
        config.ws_pair_max_attempts_per_ip,
        config.ws_pair_rate_limit_window_secs,
    );
    let (_, now_playing_rx) = watch::channel::<Option<NowPlaying>>(None);
    let profile_service = ProfileService::new(db.clone(), active_users.clone());
    let twenty_forty_eight_service = TwentyFortyEightService::new(db.clone());
    let tetris_service = TetrisService::new(db.clone());
    let chip_service = ChipService::new(db.clone());
    let rooms_service = RoomsService::new(db.clone());
    let blackjack_table_manager = BlackjackTableManager::new(chip_service.clone());
    let (blackjack_event_tx, _) = broadcast::channel(64);
    let blackjack_service = BlackjackService::new(chip_service.clone(), blackjack_event_tx);
    let sudoku_service = SudokuService::new(db.clone(), activity_tx.clone(), chip_service.clone());
    let nonogram_service =
        NonogramService::new(db.clone(), activity_tx.clone(), chip_service.clone());
    let solitaire_service =
        SolitaireService::new(db.clone(), activity_tx.clone(), chip_service.clone());
    let minesweeper_service =
        MinesweeperService::new(db.clone(), activity_tx.clone(), chip_service.clone());
    let bonsai_service = BonsaiService::new(db.clone(), activity_tx.clone());
    let dartboard_server = late_ssh::dartboard::spawn_server();
    let leaderboard_service = LeaderboardService::new(db.clone());
    State {
        conn_limit: Arc::new(Semaphore::new(config.max_conns_global)),
        conn_counts: Arc::new(Mutex::new(HashMap::<IpAddr, usize>::new())),
        active_users,
        config,
        db,
        vote_service,
        chat_service,
        notification_service,
        ai_service,
        article_service,
        showcase_service,
        profile_service,
        twenty_forty_eight_service,
        tetris_service,
        sudoku_service,
        nonogram_service,
        solitaire_service,
        minesweeper_service,
        bonsai_service,
        nonogram_library: NonogramLibrary::default(),
        chip_service,
        rooms_service,
        blackjack_table_manager,
        blackjack_service,
        dartboard_server,
        dartboard_provenance: test_dartboard_provenance(),
        leaderboard_service,
        now_playing_rx,
        activity_feed: activity_tx,
        session_registry: SessionRegistry::new(),
        paired_client_registry: PairedClientRegistry::new(),
        web_chat_registry: late_ssh::web::WebChatRegistry::new(),
        ssh_attempt_limiter,
        ws_pair_limiter,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}

pub fn make_app(db: Db, user_id: Uuid, session_token: &str) -> App {
    make_app_with_chat_service(db, user_id, session_token).0
}

pub fn make_app_with_chat_service(
    db: Db,
    user_id: Uuid,
    session_token: &str,
) -> (App, ChatService) {
    let chat_service = ChatService::new(db.clone(), NotificationService::new(db.clone()));
    let mut app = App::new(SessionConfig {
        cols: 100,
        rows: 32,
        vote_service: VoteService::new(
            db.clone(),
            "127.0.0.1:0".to_string(),
            Duration::from_secs(30 * 60),
            Arc::new(Mutex::new(HashMap::new())),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        chat_service: chat_service.clone(),
        notification_service: NotificationService::new(db.clone()),
        article_service: ArticleService::new(
            db.clone(),
            AiService::new(false, None, "gemini-3.1-pro-preview".to_string()),
            chat_service.clone(),
        ),
        showcase_service: late_ssh::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
        profile_service: ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new()))),
        twenty_forty_eight_service: TwentyFortyEightService::new(db.clone()),
        initial_2048_game: None,
        initial_2048_high_score: None,
        tetris_service: TetrisService::new(db.clone()),
        initial_tetris_game: None,
        initial_tetris_high_score: None,
        sudoku_service: SudokuService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_sudoku_games: Vec::new(),
        nonogram_service: NonogramService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_nonogram_games: Vec::new(),
        solitaire_service: SolitaireService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_solitaire_games: Vec::new(),
        minesweeper_service: MinesweeperService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_minesweeper_games: Vec::new(),
        rooms_service: RoomsService::new(db.clone()),
        blackjack_table_manager: BlackjackTableManager::new(ChipService::new(db.clone())),
        blackjack_service: BlackjackService::new(
            ChipService::new(db.clone()),
            broadcast::channel(64).0,
        ),
        dartboard_server: test_dartboard_server(),
        dartboard_provenance: test_dartboard_provenance(),
        artboard_snapshot_service: late_ssh::app::artboard::svc::ArtboardSnapshotService::new(
            db.clone(),
        ),
        username: "test-user".to_string(),
        bonsai_service: BonsaiService::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
        initial_bonsai_tree: None,
        initial_bonsai_care: None,
        nonogram_library: NonogramLibrary::default(),
        initial_chip_balance: 0,
        leaderboard_rx: None,
        web_url: "http://localhost:3000".to_string(),
        session_token: session_token.to_string(),
        session_registry: None,
        paired_client_registry: None,
        web_chat_registry: None,
        session_rx: None,
        now_playing_rx: None,
        user_id,
        is_admin: false,
        my_vote: None,
        active_users: None,
        activity_feed_rx: None,
        is_new_user: false,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        initial_theme_id: "late".to_string(),
    })
    .expect("app");
    app.skip_splash_for_tests();
    (app, chat_service)
}

pub fn make_app_with_paired_client(
    db: Db,
    user_id: Uuid,
    session_token: &str,
) -> (
    App,
    tokio::sync::mpsc::UnboundedReceiver<PairControlMessage>,
) {
    let registry = PairedClientRegistry::new();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    registry.register(session_token.to_string(), tx);

    let mut app = App::new(SessionConfig {
        cols: 100,
        rows: 32,
        vote_service: VoteService::new(
            db.clone(),
            "127.0.0.1:0".to_string(),
            Duration::from_secs(30 * 60),
            Arc::new(Mutex::new(HashMap::new())),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        chat_service: ChatService::new(db.clone(), NotificationService::new(db.clone())),
        notification_service: NotificationService::new(db.clone()),
        article_service: ArticleService::new(
            db.clone(),
            AiService::new(false, None, "gemini-3.1-pro-preview".to_string()),
            ChatService::new(db.clone(), NotificationService::new(db.clone())),
        ),
        showcase_service: late_ssh::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
        profile_service: ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new()))),
        twenty_forty_eight_service: TwentyFortyEightService::new(db.clone()),
        initial_2048_game: None,
        initial_2048_high_score: None,
        tetris_service: TetrisService::new(db.clone()),
        initial_tetris_game: None,
        initial_tetris_high_score: None,
        sudoku_service: SudokuService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_sudoku_games: Vec::new(),
        nonogram_service: NonogramService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_nonogram_games: Vec::new(),
        solitaire_service: SolitaireService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_solitaire_games: Vec::new(),
        minesweeper_service: MinesweeperService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
            ChipService::new(db.clone()),
        ),
        initial_minesweeper_games: Vec::new(),
        rooms_service: RoomsService::new(db.clone()),
        blackjack_table_manager: BlackjackTableManager::new(ChipService::new(db.clone())),
        blackjack_service: BlackjackService::new(
            ChipService::new(db.clone()),
            broadcast::channel(64).0,
        ),
        dartboard_server: test_dartboard_server(),
        dartboard_provenance: test_dartboard_provenance(),
        artboard_snapshot_service: late_ssh::app::artboard::svc::ArtboardSnapshotService::new(
            db.clone(),
        ),
        username: "test-user".to_string(),
        bonsai_service: BonsaiService::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
        initial_bonsai_tree: None,
        initial_bonsai_care: None,
        nonogram_library: NonogramLibrary::default(),
        initial_chip_balance: 0,
        leaderboard_rx: None,
        web_url: "http://localhost:3000".to_string(),
        session_token: session_token.to_string(),
        session_registry: None,
        paired_client_registry: Some(registry),
        web_chat_registry: None,
        session_rx: None,
        now_playing_rx: None,
        user_id,
        is_admin: false,
        my_vote: None,
        active_users: None,
        activity_feed_rx: None,
        is_new_user: false,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        initial_theme_id: "late".to_string(),
    })
    .expect("app");
    app.skip_splash_for_tests();
    (app, rx)
}

pub async fn wait_until<F, Fut>(mut predicate: F, label: &str)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if predicate().await {
            return;
        }
        sleep(Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for condition: {label}");
}

/// Returns [`TestDb`] alongside the app so the Postgres container outlives
/// the test body.
pub async fn chat_compose_app(name: &str) -> (TestDb, App) {
    use late_core::models::{chat_room::ChatRoom, chat_room_member::ChatRoomMember};
    use late_core::test_utils::create_test_user;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, &format!("{name}-it")).await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");

    let mut app = make_app(test_db.db.clone(), user.id, &format!("{name}-flow-it"));
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;
    (test_db, app)
}

pub async fn wait_for_render_contains(app: &mut App, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        app.tick();
        app.reset_render();
        let frame = app.render().expect("render");
        let plain = strip_ansi(&String::from_utf8_lossy(&frame));
        if plain.contains(needle) {
            return;
        }
        sleep(Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for render to contain {needle:?}");
}

pub async fn assert_render_not_contains_for(app: &mut App, needle: &str, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        app.tick();
        app.reset_render();
        let frame = app.render().expect("render");
        let plain = strip_ansi(&String::from_utf8_lossy(&frame));
        assert!(
            !plain.contains(needle),
            "render unexpectedly contained {needle:?}: {plain:?}"
        );
        sleep(Duration::from_millis(30)).await;
    }
}

/// Render one frame, tick once beforehand so async state drains, strip ANSI,
/// and return the plain-text buffer for substring/line assertions.
pub fn render_plain(app: &mut App) -> String {
    app.tick();
    app.reset_render();
    let frame = app.render().expect("render");
    strip_ansi(&String::from_utf8_lossy(&frame))
}

pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1B}' {
            out.push(ch);
            continue;
        }
        if !matches!(chars.peek(), Some('[')) {
            continue;
        }
        chars.next();
        for c in chars.by_ref() {
            if matches!(c, '\u{40}'..='\u{7E}') {
                break;
            }
        }
    }
    out
}
