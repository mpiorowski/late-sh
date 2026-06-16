#![allow(dead_code)]

use late_core::{
    api_types::NowPlaying,
    db::Db,
    rate_limit::IpRateLimiter,
    test_utils::{TestDb, test_db},
};
use late_ssh::app::activity::event::ActivityEvent;
use late_ssh::app::activity::publisher::ActivityPublisher;
use late_ssh::app::ai::svc::AiService;
use late_ssh::app::arcade::minesweeper::svc::MinesweeperService;
use late_ssh::app::arcade::nonogram::state::Library as NonogramLibrary;
use late_ssh::app::arcade::nonogram::svc::NonogramService;
use late_ssh::app::arcade::snake::svc::SnakeService;
use late_ssh::app::arcade::solitaire::svc::SolitaireService;
use late_ssh::app::arcade::sudoku::svc::SudokuService;
use late_ssh::app::arcade::tetris::svc::LaterisService;
use late_ssh::app::arcade::twenty_forty_eight::svc::TwentyFortyEightService;
use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::bonsai::svc::BonsaiService;
use late_ssh::app::chat::news::svc::ArticleService;
use late_ssh::app::chat::notifications::svc::NotificationService;
use late_ssh::app::chat::svc::ChatService;
use late_ssh::app::games::chips::svc::ChipService;
use late_ssh::app::pet::svc::PetService;
use late_ssh::app::pinstar::svc::PinstarServerRegistry;
use late_ssh::app::profile::svc::ProfileService;
use late_ssh::app::rooms::asterion::manager::AsterionRoomManager;
use late_ssh::app::rooms::blackjack::manager::BlackjackTableManager;
use late_ssh::app::rooms::blackjack::player::BlackjackPlayerDirectory;
use late_ssh::app::rooms::chess::manager::ChessTableManager;
use late_ssh::app::rooms::poker::manager::PokerTableManager;
use late_ssh::app::rooms::registry::RoomGameRegistry;
use late_ssh::app::rooms::sshattrick::manager::SshattrickRoomManager;
use late_ssh::app::rooms::svc::RoomsService;
use late_ssh::app::rooms::tictactoe::manager::TicTacToeTableManager;
use late_ssh::app::rooms::tron::manager::TronTableManager;
use late_ssh::app::state::{App, SessionConfig};
use late_ssh::app::voice::svc::{VoiceConfig, VoiceService};
use late_ssh::app::{LeaderboardService, QuestService, ShopService};
use late_ssh::authz::Permissions;
use late_ssh::config::{AiConfig, Config, WebTunnelConfig};
use late_ssh::paired_clients::{PairControlMessage, PairedClientRegistry};
use late_ssh::session::SessionRegistry;
use late_ssh::state::State;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::{Semaphore, broadcast, watch};
use tokio::time::{Duration, Instant, sleep};
use uuid::Uuid;

pub async fn new_test_db() -> TestDb {
    test_db().await
}

fn test_sudoku_games(user_id: Uuid) -> Vec<late_core::models::sudoku::Game> {
    let today = chrono::Utc::now().date_naive();
    // App flow tests do not exercise Sudoku; preloading daily boards keeps
    // app construction from spawning expensive date-dependent generators.
    [
        (
            "easy",
            "530070000600195000098000060800060003400803001700020006060000280000419005000080079",
        ),
        (
            "medium",
            "000260701680070090190004500820100040004602900050003028009300074040050036703018000",
        ),
        (
            "hard",
            "000000907000420180000705026100904000050000040000507009920108000034059000507000000",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(idx, (difficulty_key, puzzle))| {
        let mut grid = [[0u8; 9]; 9];
        let mut fixed_mask = [[false; 9]; 9];
        for (cell, byte) in puzzle.as_bytes().iter().copied().enumerate().take(81) {
            let value = byte.saturating_sub(b'0').min(9);
            let row = cell / 9;
            let col = cell % 9;
            grid[row][col] = value;
            fixed_mask[row][col] = value != 0;
        }

        late_core::models::sudoku::Game {
            id: Uuid::now_v7(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            user_id,
            mode: "daily".to_string(),
            difficulty_key: difficulty_key.to_string(),
            puzzle_date: Some(today),
            puzzle_seed: idx as i64,
            grid: serde_json::to_value(grid).expect("sudoku grid json"),
            fixed_mask: serde_json::to_value(fixed_mask).expect("sudoku fixed mask json"),
            is_game_over: false,
            score: 0,
        }
    })
    .collect()
}

fn test_dartboard_server() -> dartboard_local::ServerHandle {
    late_ssh::dartboard::spawn_server()
}

fn test_dartboard_provenance() -> late_ssh::app::artboard::provenance::SharedArtboardProvenance {
    ArtboardProvenance::default().shared()
}

fn test_room_game_registry(db: Db) -> RoomGameRegistry {
    let chip_service = ChipService::new(db.clone());
    let rooms_service = RoomsService::new(db.clone());
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(64);
    let activity_publisher = ActivityPublisher::new(db.clone(), activity_tx);
    let asterion_room_manager = AsterionRoomManager::new(
        chip_service.clone(),
        activity_publisher.clone(),
        rooms_service.clone(),
        db.clone(),
    );
    let blackjack_table_manager = BlackjackTableManager::new(
        chip_service.clone(),
        BlackjackPlayerDirectory::new(db.clone()),
        activity_publisher.clone(),
        rooms_service.clone(),
    );
    RoomGameRegistry::new(
        asterion_room_manager,
        blackjack_table_manager,
        ChessTableManager::new(
            chip_service.clone(),
            activity_publisher.clone(),
            rooms_service.clone(),
        ),
        PokerTableManager::new(
            chip_service.clone(),
            activity_publisher.clone(),
            rooms_service.clone(),
        ),
        SshattrickRoomManager::new(
            rooms_service.clone(),
            chip_service.clone(),
            activity_publisher.clone(),
            db,
        ),
        TicTacToeTableManager::new(activity_publisher.clone(), rooms_service.clone()),
        TronTableManager::new(
            chip_service,
            activity_publisher.clone(),
            rooms_service.clone(),
        ),
    )
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
        frame_drop_log_every: 100,
        ssh_max_attempts_per_ip: 30,
        ssh_rate_limit_window_secs: 60,
        ssh_proxy_protocol: false,
        ssh_proxy_trusted_cidrs: vec![],
        ws_pair_max_attempts_per_ip: 30,
        ws_pair_rate_limit_window_secs: 60,
        web_tunnel: WebTunnelConfig {
            token: "test-web-tunnel-token".to_string(),
            username: "web-demo".to_string(),
            fingerprint: "web-tunnel-demo".to_string(),
        },
        ai: AiConfig {
            enabled: false,
            api_key: None,
            model: "gemini-3.1-pro-preview".to_string(),
        },
        youtube_api_key: None,
        voice: VoiceConfig::disabled(),
        rebels_enabled: true,
        rebels_host: "frittura.org".to_string(),
        rebels_port: 3788,
        rebels_secret: String::new(),
    }
}

pub fn test_app_state(db: Db, config: Config) -> State {
    let active_users = Arc::new(Mutex::new(HashMap::new()));
    let afk_users = late_ssh::state::new_afk_users();
    let username_directory = Arc::new(Mutex::new(Arc::new(HashMap::new())));
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(64);
    let session_registry = SessionRegistry::new();
    let notification_service = NotificationService::new(db.clone());
    let chat_service = ChatService::new_with_active_users(
        db.clone(),
        notification_service.clone(),
        active_users.clone(),
    )
    .with_username_directory(username_directory.clone())
    .with_session_registry(session_registry.clone());
    let ai_service = AiService::new(false, None, "gemini-3.1-pro-preview".to_string());
    let article_service = ArticleService::new(db.clone(), ai_service.clone(), chat_service.clone());
    let feed_service = late_ssh::app::chat::feeds::svc::FeedService::new(db.clone());
    let showcase_service = late_ssh::app::chat::showcase::svc::ShowcaseService::new(db.clone());
    let work_service = late_ssh::app::chat::work::svc::WorkService::new(db.clone());
    let ssh_attempt_limiter = IpRateLimiter::new(
        config.ssh_max_attempts_per_ip,
        config.ssh_rate_limit_window_secs,
    );
    let ws_pair_limiter = IpRateLimiter::new(
        config.ws_pair_max_attempts_per_ip,
        config.ws_pair_rate_limit_window_secs,
    );
    let (_, now_playing_rx) =
        watch::channel::<std::collections::HashMap<String, NowPlaying>>(Default::default());
    let (_, radio_meta_rx) = watch::channel::<
        std::collections::HashMap<String, late_ssh::app::audio::radio_meta::svc::ArtistTitle>,
    >(Default::default());
    let profile_service = ProfileService::new(db.clone(), active_users.clone())
        .with_username_directory(username_directory.clone())
        .with_session_registry(session_registry.clone());
    let twenty_forty_eight_service = TwentyFortyEightService::new(db.clone());
    let tetris_service = LaterisService::new(db.clone());
    let snake_service = SnakeService::new(db.clone());
    let chip_service = ChipService::new(db.clone());
    let rooms_service = RoomsService::new(db.clone());
    let blackjack_player_directory = BlackjackPlayerDirectory::new(db.clone());
    let activity_publisher = ActivityPublisher::new(db.clone(), activity_tx.clone());
    let asterion_room_manager = AsterionRoomManager::new(
        chip_service.clone(),
        activity_publisher.clone(),
        rooms_service.clone(),
        db.clone(),
    );
    let blackjack_table_manager = BlackjackTableManager::new(
        chip_service.clone(),
        blackjack_player_directory.clone(),
        activity_publisher.clone(),
        rooms_service.clone(),
    );
    let sudoku_service = SudokuService::new(db.clone(), activity_tx.clone());
    let nonogram_service = NonogramService::new(db.clone(), activity_tx.clone());
    let solitaire_service = SolitaireService::new(db.clone(), activity_tx.clone());
    let minesweeper_service = MinesweeperService::new(db.clone(), activity_tx.clone());
    let bonsai_service = BonsaiService::new(db.clone(), activity_tx.clone());
    let pet_service = PetService::new(db.clone());
    let dartboard_server = late_ssh::dartboard::spawn_server();
    let leaderboard_service = LeaderboardService::new(db.clone());
    let quest_service = QuestService::new(db.clone(), activity_tx.clone());
    let shop_service = ShopService::new(db.clone());
    let ultimate_service = late_ssh::app::UltimateService::new(db.clone());
    let voice_service = VoiceService::new(config.voice.clone());
    let (room_join_feed, _) =
        broadcast::channel::<late_ssh::app::dashboard::state::DashboardRoomJoin>(64);
    State {
        conn_limit: Arc::new(Semaphore::new(config.max_conns_global)),
        conn_counts: Arc::new(Mutex::new(HashMap::<IpAddr, usize>::new())),
        active_users,
        afk_users,
        username_directory,
        config,
        db: db.clone(),
        audio_service: late_ssh::app::audio::svc::AudioService::new(
            db.clone(),
            None,
            late_ssh::paired_clients::PairedClientRegistry::new("https://audio.late.sh"),
            Arc::new(Mutex::new(HashMap::new())),
        ),
        voice_service,
        chat_service,
        notification_service,
        ai_service,
        article_service,
        feed_service,
        showcase_service,
        work_service,
        profile_service,
        twenty_forty_eight_service,
        tetris_service,
        snake_service,
        sudoku_service,
        nonogram_service,
        solitaire_service,
        minesweeper_service,
        bonsai_service,
        pet_service,
        nonogram_library: NonogramLibrary::default(),
        chip_service: chip_service.clone(),
        lateania_service: late_ssh::app::door::lateania::svc::LateaniaService::new(
            activity_publisher.clone(),
            chip_service.clone(),
            db.clone(),
        ),
        rooms_service: rooms_service.clone(),
        blackjack_table_manager: blackjack_table_manager.clone(),
        room_game_registry: RoomGameRegistry::new(
            asterion_room_manager,
            blackjack_table_manager,
            ChessTableManager::new(
                chip_service.clone(),
                activity_publisher.clone(),
                rooms_service.clone(),
            ),
            PokerTableManager::new(
                chip_service.clone(),
                activity_publisher.clone(),
                rooms_service.clone(),
            ),
            SshattrickRoomManager::new(
                rooms_service.clone(),
                chip_service.clone(),
                activity_publisher.clone(),
                db.clone(),
            ),
            TicTacToeTableManager::new(activity_publisher.clone(), rooms_service.clone()),
            TronTableManager::new(
                chip_service.clone(),
                activity_publisher.clone(),
                rooms_service.clone(),
            ),
        ),
        dartboard_server,
        dartboard_provenance: test_dartboard_provenance(),
        leaderboard_service,
        quest_service,
        shop_service,
        ultimate_service,
        now_playing_rx,
        radio_meta_rx,
        activity_feed: activity_tx,
        activity_history: Arc::new(Mutex::new(VecDeque::new())),
        room_join_feed,
        room_join_history: Arc::new(Mutex::new(VecDeque::new())),
        session_registry,
        paired_client_registry: PairedClientRegistry::new("https://audio.late.sh"),
        ssh_attempt_limiter,
        ws_pair_limiter,
        pinstar_registry: PinstarServerRegistry::new(Some(db.clone())),
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}

pub fn make_app(db: Db, user_id: Uuid, session_token: &str) -> App {
    make_app_with_chat_service(db, user_id, session_token).0
}

pub fn make_app_with_permissions(
    db: Db,
    user_id: Uuid,
    session_token: &str,
    permissions: Permissions,
) -> App {
    make_app_with_chat_service_and_permissions(db, user_id, session_token, permissions).0
}

pub fn make_app_with_chat_service(
    db: Db,
    user_id: Uuid,
    session_token: &str,
) -> (App, ChatService) {
    make_app_with_chat_service_and_permissions(db, user_id, session_token, Permissions::default())
}

fn make_app_with_chat_service_and_permissions(
    db: Db,
    user_id: Uuid,
    session_token: &str,
    permissions: Permissions,
) -> (App, ChatService) {
    let chat_service = ChatService::new(db.clone(), NotificationService::new(db.clone()));
    let activity_tx = broadcast::channel::<ActivityEvent>(64).0;
    let quest_service = QuestService::new(db.clone(), activity_tx.clone());
    let quest_snapshot_rx = quest_service.subscribe_snapshot(user_id);
    let shop_service = ShopService::new(db.clone());
    let shop_snapshot_rx = shop_service.subscribe_snapshot(user_id);
    let ultimate_service = late_ssh::app::UltimateService::new(db.clone());
    let chip_service = ChipService::new(db.clone());
    let mut app = App::new(SessionConfig {
        cols: 100,
        rows: 32,
        term: "xterm-256color".to_string(),
        audio_service: late_ssh::app::audio::svc::AudioService::new(
            db.clone(),
            None,
            late_ssh::paired_clients::PairedClientRegistry::new("https://audio.late.sh"),
            Arc::new(Mutex::new(HashMap::new())),
        ),
        voice_service: VoiceService::new(VoiceConfig::disabled()),
        chat_service: chat_service.clone(),
        notification_service: NotificationService::new(db.clone()),
        article_service: ArticleService::new(
            db.clone(),
            AiService::new(false, None, "gemini-3.1-pro-preview".to_string()),
            chat_service.clone(),
        ),
        feed_service: late_ssh::app::chat::feeds::svc::FeedService::new(db.clone()),
        showcase_service: late_ssh::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
        work_service: late_ssh::app::chat::work::svc::WorkService::new(db.clone()),
        profile_service: ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new()))),
        twenty_forty_eight_service: TwentyFortyEightService::new(db.clone()),
        initial_2048_game: None,
        initial_2048_high_score: None,
        tetris_service: LaterisService::new(db.clone()),
        snake_service: SnakeService::new(db.clone()),
        initial_tetris_game: None,
        initial_snake_game: None,
        initial_tetris_high_score: None,
        initial_snake_high_score: None,
        sudoku_service: SudokuService::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
        initial_sudoku_games: test_sudoku_games(user_id),
        nonogram_service: NonogramService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        initial_nonogram_games: Vec::new(),
        solitaire_service: SolitaireService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        initial_solitaire_games: Vec::new(),
        minesweeper_service: MinesweeperService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        initial_minesweeper_games: Vec::new(),
        lateania_service: late_ssh::app::door::lateania::svc::LateaniaService::new(
            ActivityPublisher::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
            chip_service,
            db.clone(),
        ),
        rooms_service: RoomsService::new(db.clone()),
        room_game_registry: test_room_game_registry(db.clone()),
        dartboard_server: test_dartboard_server(),
        dartboard_provenance: test_dartboard_provenance(),
        artboard_snapshot_service: late_ssh::app::artboard::svc::ArtboardSnapshotService::new(
            db.clone(),
        ),
        username: "test-user".to_string(),
        bonsai_service: BonsaiService::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
        initial_bonsai_tree: None,
        initial_bonsai_care: None,
        initial_bonsai_v2_tree: None,
        pet_service: PetService::new(db.clone()),
        initial_pet: None,
        quest_service,
        quest_snapshot_rx,
        shop_service,
        shop_snapshot_rx,
        ultimate_service,
        initial_ultimate_cooldowns: Vec::new(),
        nonogram_library: NonogramLibrary::default(),
        initial_chip_balance: 0,
        leaderboard_rx: None,
        web_url: "http://localhost:3000".to_string(),
        rebels_enabled: true,
        rebels_host: "frittura.org".to_string(),
        rebels_port: 3788,
        rebels_secret: String::new(),
        session_token: session_token.to_string(),
        session_registry: None,
        paired_client_registry: None,
        pinstar_registry: PinstarServerRegistry::new(Some(db.clone())),
        session_rx: None,
        now_playing_rx: None,
        radio_meta_rx: None,
        user_id,
        permissions,
        artboard_banned: false,
        artboard_ban_expires_at: None,
        active_users: None,
        afk_users: late_ssh::state::new_afk_users(),
        username_directory: None,
        activity_feed_rx: None,
        initial_activity: VecDeque::new(),
        room_join_rx: None,
        initial_room_joins: VecDeque::new(),
        initial_announcements: None,
        is_new_user: false,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        initial_theme_id: "contrast".to_string(),
        initial_audio_source: late_core::models::user::AudioSource::default(),
        initial_icecast_stream: late_core::models::user::IcecastStream::default(),
        initial_radio_station: late_core::models::user::RadioStation::default(),
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
    let registry = PairedClientRegistry::new("https://audio.late.sh");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    registry.register(
        session_token.to_string(),
        tx,
        uuid::Uuid::now_v7(),
        late_core::models::user::AudioSource::default(),
    );
    let activity_tx = broadcast::channel::<ActivityEvent>(64).0;
    let quest_service = QuestService::new(db.clone(), activity_tx.clone());
    let quest_snapshot_rx = quest_service.subscribe_snapshot(user_id);
    let shop_service = ShopService::new(db.clone());
    let shop_snapshot_rx = shop_service.subscribe_snapshot(user_id);
    let ultimate_service = late_ssh::app::UltimateService::new(db.clone());
    let chip_service = ChipService::new(db.clone());

    let mut app = App::new(SessionConfig {
        cols: 100,
        rows: 32,
        term: "xterm-256color".to_string(),
        audio_service: late_ssh::app::audio::svc::AudioService::new(
            db.clone(),
            None,
            late_ssh::paired_clients::PairedClientRegistry::new("https://audio.late.sh"),
            Arc::new(Mutex::new(HashMap::new())),
        ),
        voice_service: VoiceService::new(VoiceConfig::disabled()),
        chat_service: ChatService::new(db.clone(), NotificationService::new(db.clone())),
        notification_service: NotificationService::new(db.clone()),
        article_service: ArticleService::new(
            db.clone(),
            AiService::new(false, None, "gemini-3.1-pro-preview".to_string()),
            ChatService::new(db.clone(), NotificationService::new(db.clone())),
        ),
        feed_service: late_ssh::app::chat::feeds::svc::FeedService::new(db.clone()),
        showcase_service: late_ssh::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
        work_service: late_ssh::app::chat::work::svc::WorkService::new(db.clone()),
        profile_service: ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new()))),
        twenty_forty_eight_service: TwentyFortyEightService::new(db.clone()),
        initial_2048_game: None,
        initial_2048_high_score: None,
        tetris_service: LaterisService::new(db.clone()),
        snake_service: SnakeService::new(db.clone()),
        initial_tetris_game: None,
        initial_snake_game: None,
        initial_tetris_high_score: None,
        initial_snake_high_score: None,
        sudoku_service: SudokuService::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
        initial_sudoku_games: test_sudoku_games(user_id),
        nonogram_service: NonogramService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        initial_nonogram_games: Vec::new(),
        solitaire_service: SolitaireService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        initial_solitaire_games: Vec::new(),
        minesweeper_service: MinesweeperService::new(
            db.clone(),
            broadcast::channel::<ActivityEvent>(64).0,
        ),
        initial_minesweeper_games: Vec::new(),
        lateania_service: late_ssh::app::door::lateania::svc::LateaniaService::new(
            ActivityPublisher::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
            chip_service,
            db.clone(),
        ),
        rooms_service: RoomsService::new(db.clone()),
        room_game_registry: test_room_game_registry(db.clone()),
        dartboard_server: test_dartboard_server(),
        dartboard_provenance: test_dartboard_provenance(),
        artboard_snapshot_service: late_ssh::app::artboard::svc::ArtboardSnapshotService::new(
            db.clone(),
        ),
        username: "test-user".to_string(),
        bonsai_service: BonsaiService::new(db.clone(), broadcast::channel::<ActivityEvent>(64).0),
        initial_bonsai_tree: None,
        initial_bonsai_care: None,
        initial_bonsai_v2_tree: None,
        pet_service: PetService::new(db.clone()),
        initial_pet: None,
        quest_service,
        quest_snapshot_rx,
        shop_service,
        shop_snapshot_rx,
        ultimate_service,
        initial_ultimate_cooldowns: Vec::new(),
        nonogram_library: NonogramLibrary::default(),
        initial_chip_balance: 0,
        leaderboard_rx: None,
        web_url: "http://localhost:3000".to_string(),
        rebels_enabled: true,
        rebels_host: "frittura.org".to_string(),
        rebels_port: 3788,
        rebels_secret: String::new(),
        session_token: session_token.to_string(),
        session_registry: None,
        paired_client_registry: Some(registry),
        pinstar_registry: PinstarServerRegistry::new(Some(db.clone())),
        session_rx: None,
        now_playing_rx: None,
        radio_meta_rx: None,
        user_id,
        permissions: Permissions::default(),
        artboard_banned: false,
        artboard_ban_expires_at: None,
        active_users: None,
        afk_users: late_ssh::state::new_afk_users(),
        username_directory: None,
        activity_feed_rx: None,
        initial_activity: VecDeque::new(),
        room_join_rx: None,
        initial_room_joins: VecDeque::new(),
        initial_announcements: None,
        is_new_user: false,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        initial_icecast_stream: late_core::models::user::IcecastStream::default(),
        initial_radio_station: late_core::models::user::RadioStation::default(),
        initial_theme_id: "contrast".to_string(),
        initial_audio_source: late_core::models::user::AudioSource::default(),
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
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");

    let mut app = make_app(test_db.db.clone(), user.id, &format!("{name}-flow-it"));
    wait_for_render_contains(&mut app, "lounge").await;
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Compose (Enter send").await;
    (test_db, app)
}

pub async fn wait_for_render_contains(app: &mut App, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last_plain = String::new();
    while Instant::now() < deadline {
        app.tick();
        app.reset_render();
        let frame = app.render().expect("render");
        let plain = strip_ansi(&String::from_utf8_lossy(&frame));
        if plain.contains(needle) {
            return;
        }
        last_plain = plain;
        sleep(Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for render to contain {needle:?}; last render:\n{last_plain}");
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
