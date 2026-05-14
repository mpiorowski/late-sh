use crate::app::activity::event::ActivityEvent;
use crate::app::ai::svc::AiService;
use crate::app::arcade::chips::svc::ChipService;
use crate::app::arcade::minesweeper::svc::MinesweeperService;
use crate::app::arcade::nonogram::state::Library as NonogramLibrary;
use crate::app::arcade::nonogram::svc::NonogramService;
use crate::app::arcade::snake::svc::SnakeService;
use crate::app::arcade::solitaire::svc::SolitaireService;
use crate::app::arcade::sudoku::svc::SudokuService;
use crate::app::arcade::tetris::svc::TetrisService;
use crate::app::arcade::twenty_forty_eight::svc::TwentyFortyEightService;
use crate::app::artboard::provenance::SharedArtboardProvenance;
use crate::app::audio::svc::AudioService;
use crate::app::bonsai::svc::BonsaiService;
use crate::app::chat::feeds::svc::FeedService;
use crate::app::chat::news::svc::ArticleService;
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::showcase::svc::ShowcaseService;
use crate::app::chat::svc::ChatService;
use crate::app::chat::work::svc::WorkService;
use crate::app::hub::svc::LeaderboardService;
use crate::app::profile::svc::ProfileService;
use crate::app::rooms::blackjack::manager::BlackjackTableManager;
use crate::app::rooms::registry::RoomGameRegistry;
use crate::app::rooms::svc::RoomsService;
use crate::app::vote::svc::VoteService;
use crate::config::Config;
use crate::session::{PairedClientRegistry, SessionRegistry};
use crate::web::WebChatRegistry;
use late_core::{api_types::NowPlaying, db::Db, rate_limit::IpRateLimiter};
use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::{Semaphore, broadcast, watch};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ActiveSession {
    pub token: String,
    pub fingerprint: Option<String>,
    pub peer_ip: Option<IpAddr>,
}

#[derive(Clone, Debug)]
pub struct ActiveUser {
    pub username: String,
    pub fingerprint: Option<String>,
    pub peer_ip: Option<IpAddr>,
    pub sessions: Vec<ActiveSession>,
    pub connection_count: usize,
    pub last_login_at: Instant,
}

pub type ActiveUsers = Arc<Mutex<HashMap<Uuid, ActiveUser>>>;
pub type ActivityHistory = Arc<Mutex<VecDeque<ActivityEvent>>>;

#[derive(Clone)]
pub struct State {
    pub config: Config,
    pub db: Db,
    pub ai_service: AiService,
    pub audio_service: AudioService,
    pub vote_service: VoteService,
    pub chat_service: ChatService,
    pub notification_service: NotificationService,
    pub article_service: ArticleService,
    pub feed_service: FeedService,
    pub showcase_service: ShowcaseService,
    pub work_service: WorkService,
    pub profile_service: ProfileService,
    pub twenty_forty_eight_service: TwentyFortyEightService,
    pub tetris_service: TetrisService,
    pub snake_service: SnakeService,
    pub sudoku_service: SudokuService,
    pub nonogram_service: NonogramService,
    pub solitaire_service: SolitaireService,
    pub minesweeper_service: MinesweeperService,
    pub bonsai_service: BonsaiService,
    pub nonogram_library: NonogramLibrary,
    pub chip_service: ChipService,
    pub rooms_service: RoomsService,
    pub blackjack_table_manager: BlackjackTableManager,
    pub room_game_registry: RoomGameRegistry,
    pub dartboard_server: dartboard_local::ServerHandle,
    pub dartboard_provenance: SharedArtboardProvenance,
    pub leaderboard_service: LeaderboardService,
    pub conn_limit: Arc<Semaphore>,
    pub conn_counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    pub active_users: ActiveUsers,
    pub activity_feed: broadcast::Sender<ActivityEvent>,
    pub activity_history: ActivityHistory,
    pub now_playing_rx: watch::Receiver<Option<NowPlaying>>,
    pub session_registry: SessionRegistry,
    pub paired_client_registry: PairedClientRegistry,
    pub web_chat_registry: WebChatRegistry,
    pub ssh_attempt_limiter: IpRateLimiter,
    pub ws_pair_limiter: IpRateLimiter,
    pub is_draining: Arc<std::sync::atomic::AtomicBool>,
}
