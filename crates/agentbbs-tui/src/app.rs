//! The AgentBBS terminal application state machine.
//!
//! [`App`] owns an anonymous session (an ephemeral [`Identity`]) and a handle
//! to the capability-enforcing [`Bbs`] service. It is backend-agnostic: it
//! renders into any [`ratatui::Frame`] and consumes [`crossterm`] key events,
//! so it can be unit-tested with a `TestBackend` and driven for real over an
//! SSH PTY or the local terminal.

use std::sync::Arc;

use std::time::Instant;

use agentbbs_arena::{Arena, BenchmarkId, RunResult, Submission};
use agentbbs_core::caps::Caps;
use agentbbs_core::identity::Identity;
use agentbbs_core::market::{Listing, ListingBody, ListingKind, Market};
use agentbbs_core::presence::Presence;
use agentbbs_core::report::MemoryReporter;
use agentbbs_core::{Board, Bbs, Message, MemoryStore, Role, Store};

/// Which screen is currently focused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// The dial-up connect splash.
    Splash,
    /// The main menu (lightbar of lettered commands).
    Main,
    /// List of message bases (boards).
    Boards,
    /// Reading messages within a board.
    Read,
    /// Composing a new message.
    Compose,
    /// Who's online (sessions/agents).
    Who,
    /// Door games / external programs hub.
    Doors,
    /// Benchmark competition arena + leaderboard.
    Arena,
    /// Marketplace of signed listings.
    Market,
    /// Federation status panel.
    Federation,
    /// Sysop reporting dashboard.
    Sysop,
    /// Sign-off screen.
    Goodbye,
}

/// The lettered main-menu commands, in display order.
pub const MENU: &[(char, &str, Screen)] = &[
    ('M', "Message Bases", Screen::Boards),
    ('W', "Who's Online", Screen::Who),
    ('D', "Door Games", Screen::Doors),
    ('A', "Arena (Benchmarks)", Screen::Arena),
    ('K', "Marketplace", Screen::Market),
    ('F', "Federation", Screen::Federation),
    ('S', "Sysop Report", Screen::Sysop),
    ('G', "Goodbye / Log Off", Screen::Goodbye),
];

/// Which compose field has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposeField {
    /// The subject line.
    Subject,
    /// The message body.
    Body,
}

/// What the event loop should do after handling a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Keep running.
    Continue,
    /// Tear down and exit.
    Quit,
}

/// The anonymous session for the connected caller.
pub struct Session {
    /// The ephemeral keypair minted for this connection.
    pub identity: Identity,
    /// A cosmetic, unauthenticated handle.
    pub handle: String,
    /// Capabilities granted to this session.
    pub caps: Caps,
}

/// The full application state.
pub struct App {
    /// The capability-enforcing BBS service.
    pub bbs: Bbs,
    /// Live operational events for the sysop panel.
    pub reporter: Arc<MemoryReporter>,
    /// This caller's anonymous session.
    pub session: Session,
    /// Current screen.
    pub screen: Screen,
    /// Highlighted main-menu row.
    pub menu_index: usize,
    /// Cached board list.
    pub boards: Vec<Board>,
    /// Highlighted board row.
    pub board_index: usize,
    /// Slug of the board being read.
    pub current_board: Option<String>,
    /// Cached messages for the current board.
    pub messages: Vec<Message>,
    /// Highlighted/scrolled message row.
    pub read_index: usize,
    /// Compose: subject buffer.
    pub compose_subject: String,
    /// Compose: body buffer.
    pub compose_body: String,
    /// Compose: focused field.
    pub compose_field: ComposeField,
    /// The benchmark competition arena + leaderboard.
    pub arena: Arena,
    /// Highlighted benchmark row in the Arena screen.
    pub arena_index: usize,
    /// Node-shared presence registry (who is online across all sessions).
    pub presence: Arc<Presence>,
    /// Signed marketplace listings shown in the Marketplace screen.
    pub market: Market,
    /// Monotonic clock base for presence heartbeats.
    pub started: Instant,
    /// One-line status / error message.
    pub status: String,
    /// Whether the app wants to quit.
    pub should_quit: bool,
}

impl Drop for App {
    fn drop(&mut self) {
        // Leave the shared presence registry when the session ends.
        self.presence.leave(&self.session.identity.id());
    }
}

impl App {
    /// Build an app over an arbitrary [`Store`], minting a fresh anonymous
    /// session and seeding the default boards if the store is empty.
    pub fn new(store: Arc<dyn Store>) -> Self {
        App::with_presence(store, Arc::new(Presence::default()))
    }

    /// Build an app sharing a node-wide [`Presence`] registry, so every session
    /// on the node (each SSH connection) sees the others in Who's Online.
    pub fn with_presence(store: Arc<dyn Store>, presence: Arc<Presence>) -> Self {
        let (bbs, reporter) = Bbs::with_memory_reporter(store);
        let identity = Identity::generate();
        let session = Session {
            handle: format!("agent-{}", identity.id().short()),
            caps: Role::Agent.caps(),
            identity,
        };

        let mut app = App {
            bbs,
            reporter,
            session,
            screen: Screen::Splash,
            menu_index: 0,
            boards: Vec::new(),
            board_index: 0,
            current_board: None,
            messages: Vec::new(),
            read_index: 0,
            compose_subject: String::new(),
            compose_body: String::new(),
            compose_field: ComposeField::Subject,
            arena: Arena::new(),
            arena_index: 0,
            presence,
            market: Market::new(),
            started: Instant::now(),
            status: "Connected. Press ENTER.".into(),
            should_quit: false,
        };
        app.seed_defaults();
        app.seed_arena();
        app.seed_market();
        app.heartbeat();
        app.refresh_boards();
        app
    }

    /// Monotonic milliseconds since this session started (presence clock).
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Refresh our entry in the shared presence registry.
    pub fn heartbeat(&self) {
        self.presence
            .heartbeat(self.session.identity.id(), &self.session.handle, false, self.now_ms());
    }

    /// Seed a few signed marketplace listings so the catalogue is alive.
    fn seed_market(&mut self) {
        let listings: &[(ListingKind, &str, &str, &str, u64)] = &[
            (ListingKind::Plugin, "echo-door", "Echo Door", "A tiny WASM door that echoes/uppercases input.", 0),
            (ListingKind::Agent, "graybeard", "Graybeard Agent", "A burned-out sysadmin persona that reviews your code.", 25),
            (ListingKind::Theme, "amber-crt", "Amber CRT", "A phosphor-amber retro theme.", 5),
            (ListingKind::Benchmark, "cve-pack-2", "CVE Pack II", "Ten extra critical CVEs for the Arena.", 40),
        ];
        for (kind, sku, title, desc, price) in listings {
            let id = Identity::generate();
            let body = ListingBody {
                sku: (*sku).into(),
                kind: *kind,
                title: (*title).into(),
                description: (*desc).into(),
                price: *price,
                seller: id.id(),
                handle: "agentics".into(),
                artifact_hash: agentbbs_core::market::artifact_hash(title.as_bytes()),
                created_at: chrono::Utc::now(),
            };
            if let Ok(listing) = Listing::sign(&id, body) {
                let _ = self.market.publish(listing);
            }
        }
    }

    /// Seed the arena with a few demo competitors so the leaderboard is alive.
    /// Each is a freshly-generated anonymous identity signing a CVE-Bench run.
    fn seed_arena(&mut self) {
        let demo = [
            ("claude-opus-4.8", 0.80, 32u32),
            ("gpt-frontier", 0.55, 22),
            ("graybeard-agent", 0.30, 12),
            ("script-kiddie", 0.13, 5),
        ];
        for (handle, score, passed) in demo {
            let id = Identity::generate();
            let result = RunResult {
                benchmark: BenchmarkId("cve-bench".into()),
                competitor: id.id(),
                handle: handle.into(),
                score,
                passed,
                total: 40,
                harness: "ruflo@3.5".into(),
                at: chrono::Utc::now(),
                detail: serde_json::Value::Null,
            };
            if let Ok(sub) = Submission::sign(&id, result) {
                let _ = self.arena.submit(sub);
            }
        }
    }

    /// Convenience constructor over an in-memory store.
    pub fn in_memory() -> Self {
        App::new(Arc::new(MemoryStore::new()))
    }

    /// Seed the canonical boards once, founded by a system identity.
    fn seed_defaults(&mut self) {
        if !self.bbs.list_boards(Caps::READ).map(|b| b.is_empty()).unwrap_or(true) {
            return;
        }
        let sys = Identity::generate();
        let sys_caps = Role::Sysop.caps();
        for (slug, title, desc) in [
            ("general", "General Chat", "Open floor for agents and humans."),
            ("agents.dev", "Agent Development", "Building, debugging, and orchestrating agents."),
            ("marketplace", "Marketplace", "Plugins, agents, and boards for sale or trade."),
            ("federation", "Federation Hall", "Cross-node announcements and peering."),
        ] {
            let mut b = Board::new(slug, title, sys.id());
            b.description = desc.into();
            let _ = self.bbs.create_board(sys_caps, b);
        }
    }

    /// Reload the board list from the store.
    pub fn refresh_boards(&mut self) {
        self.boards = self.bbs.list_boards(Caps::READ).unwrap_or_default();
        if self.board_index >= self.boards.len() {
            self.board_index = self.boards.len().saturating_sub(1);
        }
    }

    /// Open the board at `board_index` and load its messages.
    pub fn open_selected_board(&mut self) {
        if let Some(board) = self.boards.get(self.board_index) {
            let slug = board.slug.clone();
            self.messages = self.bbs.read_board(Caps::READ, &slug, 200).unwrap_or_default();
            self.read_index = self.messages.len().saturating_sub(1);
            self.current_board = Some(slug);
            self.screen = Screen::Read;
        }
    }

    /// Post the in-progress compose buffer to the current board.
    pub fn submit_compose(&mut self) {
        let Some(slug) = self.current_board.clone() else {
            self.status = "No board selected.".into();
            return;
        };
        if self.compose_body.trim().is_empty() {
            self.status = "Cannot post an empty message.".into();
            return;
        }
        let subject = if self.compose_subject.trim().is_empty() {
            "(no subject)".to_string()
        } else {
            self.compose_subject.clone()
        };
        let body = agentbbs_core::MessageBody {
            board: slug.clone(),
            parent: None,
            subject,
            body: self.compose_body.clone(),
            author: self.session.identity.id(),
            handle: self.session.handle.clone(),
            created_at: chrono::Utc::now(),
        };
        match Message::sign(&self.session.identity, body)
            .and_then(|m| self.bbs.post(self.session.caps, m))
        {
            Ok(_) => {
                self.compose_subject.clear();
                self.compose_body.clear();
                self.compose_field = ComposeField::Subject;
                self.messages = self.bbs.read_board(Caps::READ, &slug, 200).unwrap_or_default();
                self.read_index = self.messages.len().saturating_sub(1);
                self.status = "Message posted and signed.".into();
                self.screen = Screen::Read;
            }
            Err(e) => self.status = format!("Post failed: {e}"),
        }
    }
}
