use crate::app::ai::svc::AiService;
use crate::app::artboard::provenance::SharedArtboardProvenance;
use crate::app::bonsai::svc::BonsaiService;
use crate::app::chat::feeds::svc::FeedService;
use crate::app::chat::news::svc::ArticleService;
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::showcase::svc::ShowcaseService;
use crate::app::chat::svc::ChatService;
use crate::app::chat::work::svc::WorkService;
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
use crate::app::rooms::blackjack::manager::BlackjackTableManager;
use crate::app::rooms::registry::RoomGameRegistry;
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
    time::{Duration, Instant},
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

const CHALLENGE_TTL: Duration = Duration::from_secs(60);
const WS_TICKET_TTL: Duration = Duration::from_secs(30);

/// In-memory store for short-lived auth nonces issued by `GET /api/native/challenge`.
#[derive(Clone, Default)]
pub struct NativeChallengeStore {
    inner: Arc<Mutex<HashMap<String, Instant>>>,
}

impl NativeChallengeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a new nonce, storing it with a 60-second TTL. Returns the nonce.
    pub fn issue(&self, nonce: String) -> String {
        let expiry = Instant::now() + CHALLENGE_TTL;
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, exp| *exp > Instant::now());
        map.insert(nonce.clone(), expiry);
        nonce
    }

    /// Remove and return whether the nonce was valid (present and not expired).
    pub fn consume(&self, nonce: &str) -> bool {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match map.remove(nonce) {
            Some(exp) => exp > Instant::now(),
            None => false,
        }
    }
}

/// One-time short-lived tickets for WebSocket authentication.
/// Minted by `GET /api/native/ws-ticket` (requires bearer auth), consumed on WS connect.
#[derive(Clone, Default)]
pub struct NativeWsTicketStore {
    inner: Arc<Mutex<HashMap<String, (Uuid, String, Instant)>>>,
}

impl NativeWsTicketStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a ticket valid for `WS_TICKET_TTL`. Returns the ticket string.
    pub fn mint(&self, user_id: Uuid, username: String) -> String {
        let ticket = crate::session::new_session_token();
        let expiry = Instant::now() + WS_TICKET_TTL;
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, (_, _, exp)| *exp > Instant::now());
        map.insert(ticket.clone(), (user_id, username, expiry));
        ticket
    }

    /// Consume and validate a ticket. Returns `(user_id, username)` if valid.
    pub fn consume(&self, ticket: &str) -> Option<(Uuid, String)> {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match map.remove(ticket) {
            Some((user_id, username, exp)) if exp > Instant::now() => Some((user_id, username)),
            _ => None,
        }
    }
}

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
    pub feed_service: FeedService,
    pub showcase_service: ShowcaseService,
    pub work_service: WorkService,
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
    pub room_game_registry: RoomGameRegistry,
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
    pub native_challenges: NativeChallengeStore,
    pub native_ws_tickets: NativeWsTicketStore,
    pub native_challenge_limiter: IpRateLimiter,
    pub native_token_limiter: IpRateLimiter,
    pub native_ws_limiter: IpRateLimiter,
    pub is_draining: Arc<std::sync::atomic::AtomicBool>,
}
