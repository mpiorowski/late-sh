use crate::app::activity::event::ActivityEvent;
use crate::app::ai::svc::AiService;
use crate::app::arcade::le_word::svc::LeWordService;
use crate::app::arcade::minesweeper::svc::MinesweeperService;
use crate::app::arcade::nonogram::state::Library as NonogramLibrary;
use crate::app::arcade::nonogram::svc::NonogramService;
use crate::app::arcade::rubiks_cube::svc::RubiksCubeService;
use crate::app::arcade::snake::svc::SnakeService;
use crate::app::arcade::solitaire::svc::SolitaireService;
use crate::app::arcade::sudoku::svc::SudokuService;
use crate::app::arcade::tetris::svc::LaterisService;
use crate::app::arcade::traffic::svc::TrafficService;
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
use crate::app::games::chips::svc::ChipService;
use crate::app::hub::dailies::svc::QuestService;
use crate::app::hub::shop::svc::ShopService;
use crate::app::hub::svc::LeaderboardService;
use crate::app::pet::svc::PetService;
use crate::app::profile::svc::ProfileService;
use crate::app::voice::svc::VoiceService;
use crate::config::Config;
use crate::paired_clients::PairedClientRegistry;
use crate::session::SessionRegistry;
use crate::usernames::UsernameDirectory;
use late_core::{
    MutexRecover, api_types::NowPlaying, db::Db, models::user::AudioSource,
    rate_limit::IpRateLimiter,
};
use std::{
    collections::{HashMap, HashSet},
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
    /// Session-local away state set by `/brb`.
    pub afk: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ActiveUser {
    pub username: String,
    pub fingerprint: Option<String>,
    pub peer_ip: Option<IpAddr>,
    pub audio_source: AudioSource,
    pub sessions: Vec<ActiveSession>,
    pub connection_count: usize,
    pub last_login_at: Instant,
}

pub type ActiveUsers = Arc<Mutex<HashMap<Uuid, ActiveUser>>>;
pub type AfkUsers = Arc<Mutex<Arc<HashSet<Uuid>>>>;

pub fn new_afk_users() -> AfkUsers {
    Arc::new(Mutex::new(Arc::new(HashSet::new())))
}

/// Connected humans only: the always-on bots (@bartender, @graybeard, @bot)
/// register with no fingerprint and are excluded, matching the clubhouse
/// headcount.
pub fn online_human_count(active_users: &ActiveUsers) -> usize {
    active_users
        .lock_recover()
        .values()
        .filter(|user| user.fingerprint.is_some())
        .count()
}

pub fn afk_users_snapshot(afk_users: &AfkUsers) -> Arc<HashSet<Uuid>> {
    Arc::clone(&afk_users.lock_recover())
}

pub fn set_afk_user(afk_users: &AfkUsers, user_id: Uuid, is_afk: bool) {
    let mut guard = afk_users.lock_recover();
    if guard.contains(&user_id) == is_afk {
        return;
    }
    let users = Arc::make_mut(&mut *guard);
    if is_afk {
        users.insert(user_id);
    } else {
        users.remove(&user_id);
    }
}

#[derive(Clone)]
pub struct State {
    pub config: Config,
    pub db: Db,
    pub ai_service: AiService,
    pub audio_service: AudioService,
    pub voice_service: VoiceService,
    pub chat_service: ChatService,
    pub notification_service: NotificationService,
    pub article_service: ArticleService,
    pub feed_service: FeedService,
    pub showcase_service: ShowcaseService,
    pub work_service: WorkService,
    pub profile_service: ProfileService,
    pub twenty_forty_eight_service: TwentyFortyEightService,
    pub tetris_service: LaterisService,
    pub snake_service: SnakeService,
    pub traffic_service: TrafficService,
    pub rubiks_cube_service: RubiksCubeService,
    pub le_word_service: LeWordService,
    pub sudoku_service: SudokuService,
    pub nonogram_service: NonogramService,
    pub solitaire_service: SolitaireService,
    pub minesweeper_service: MinesweeperService,
    pub bonsai_service: BonsaiService,
    pub pet_service: PetService,
    pub nonogram_library: NonogramLibrary,
    pub chip_service: ChipService,
    pub lateania_service: crate::app::door::lateania::svc::LateaniaService,
    pub greendragon_service: crate::app::door::greendragon::svc::GreenDragonService,
    pub arcade_handle_service: crate::app::door::arcade::ArcadeHandleService,
    pub daily_service: crate::app::lobby::daily::svc::DailyService,
    pub house_registry: crate::app::lobby::house::registry::HouseTableRegistry,
    pub dartboard_server: dartboard_local::ServerHandle,
    pub dartboard_provenance: SharedArtboardProvenance,
    pub leaderboard_service: LeaderboardService,
    pub quest_service: QuestService,
    pub shop_service: ShopService,
    pub ultimate_service: crate::app::UltimateService,
    pub conn_limit: Arc<Semaphore>,
    pub conn_counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Concurrent `/api/ws/pair` sockets per IP. Separate from `conn_counts`
    /// so pair-socket floods and SSH connections cap independently.
    pub pair_ws_counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    pub active_users: ActiveUsers,
    /// Process-global clubhouse presence: who sits where, who is walking.
    pub clubhouse_lobby: crate::app::clubhouse::lobby::SharedLobby,
    pub afk_users: AfkUsers,
    pub username_directory: UsernameDirectory,
    /// Live 24h username effects (snapshot-swap; seeded and written by
    /// `ShopService`, resolved per session in the tick loop).
    pub flair_directory: crate::app::common::username_effect::NameFlairDirectory,
    pub activity_feed: broadcast::Sender<ActivityEvent>,
    pub now_playing_rx: watch::Receiver<HashMap<String, NowPlaying>>,
    pub radio_meta_rx:
        watch::Receiver<HashMap<String, crate::app::audio::radio_meta::svc::ArtistTitle>>,
    pub worldcup_service: crate::app::worldcup::svc::WorldCupService,
    pub session_registry: SessionRegistry,
    pub paired_client_registry: PairedClientRegistry,
    pub irc_registry: crate::ircd::registry::IrcRegistry,
    pub ssh_attempt_limiter: IpRateLimiter,
    pub ws_pair_limiter: IpRateLimiter,
    pub pinstar_registry: crate::app::pinstar::svc::PinstarServerRegistry,
    pub is_draining: Arc<std::sync::atomic::AtomicBool>,
}
