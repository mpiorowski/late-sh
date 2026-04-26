use crate::app::ai::svc::AiService;
use crate::app::artboard::provenance::SharedArtboardProvenance;
use crate::app::bonsai::svc::BonsaiService;
use crate::app::chat::news::svc::ArticleService;
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::showcase::svc::ShowcaseService;
use crate::app::chat::svc::ChatService;
use crate::app::games::chips::svc::ChipService;
use crate::app::games::leaderboard::svc::LeaderboardService;
use crate::app::games::minesweeper::svc::MinesweeperService;
use crate::app::games::nonogram::state::Library as NonogramLibrary;
use crate::app::games::nonogram::svc::NonogramService;
use crate::app::games::solitaire::svc::SolitaireService;
use crate::app::games::sudoku::svc::SudokuService;
use crate::app::games::tetris::svc::TetrisService;
use crate::app::games::twenty_forty_eight::svc::TwentyFortyEightService;
use crate::app::profile::svc::ProfileService;
use crate::app::rooms::blackjack::{manager::BlackjackTableManager, svc::BlackjackService};
use crate::app::rooms::svc::RoomsService;
use crate::app::vote::svc::VoteService;
use crate::config::Config;
use crate::session::{PairedClientRegistry, SessionRegistry};
use crate::web::WebChatRegistry;
use late_core::{api_types::NowPlaying, db::Db, rate_limit::IpRateLimiter};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::{Semaphore, broadcast, watch};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ActiveUser {
    pub username: String,
    pub connection_count: usize,
    pub last_login_at: Instant,
}

pub type ActiveUsers = Arc<Mutex<HashMap<Uuid, ActiveUser>>>;

#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub username: String,
    pub action: String, // "voted Jazz", "joined", "sent a message"
    pub at: Instant,
}

#[derive(Clone)]
pub struct State {
    pub config: Config,
    pub db: Db,
    pub ai_service: AiService,
    pub vote_service: VoteService,
    pub chat_service: ChatService,
    pub notification_service: NotificationService,
    pub article_service: ArticleService,
    pub showcase_service: ShowcaseService,
    pub profile_service: ProfileService,
    pub twenty_forty_eight_service: TwentyFortyEightService,
    pub tetris_service: TetrisService,
    pub sudoku_service: SudokuService,
    pub nonogram_service: NonogramService,
    pub solitaire_service: SolitaireService,
    pub minesweeper_service: MinesweeperService,
    pub bonsai_service: BonsaiService,
    pub nonogram_library: NonogramLibrary,
    pub chip_service: ChipService,
    pub rooms_service: RoomsService,
    pub blackjack_table_manager: BlackjackTableManager,
    pub blackjack_service: BlackjackService,
    pub dartboard_server: dartboard_local::ServerHandle,
    pub dartboard_provenance: SharedArtboardProvenance,
    pub leaderboard_service: LeaderboardService,
    pub conn_limit: Arc<Semaphore>,
    pub conn_counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    pub active_users: ActiveUsers,
    pub activity_feed: broadcast::Sender<ActivityEvent>,
    pub now_playing_rx: watch::Receiver<Option<NowPlaying>>,
    pub session_registry: SessionRegistry,
    pub paired_client_registry: PairedClientRegistry,
    pub web_chat_registry: WebChatRegistry,
    pub ssh_attempt_limiter: IpRateLimiter,
    pub ws_pair_limiter: IpRateLimiter,
    pub is_draining: Arc<std::sync::atomic::AtomicBool>,
}
