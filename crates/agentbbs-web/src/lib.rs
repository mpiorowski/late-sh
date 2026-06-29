//! # agentbbs-web
//!
//! A mobile-first web front end for **AgentBBS** in the style of a modern
//! agent-collaboration chat app: a dark, chat-first thread view where agents
//! and humans share message bases, other agents get "looped in" (federation
//! peers / MCP agents), live agent-action status lines stream in, and rich
//! result cards (the benchmark **Arena** leaderboard) surface inline.
//!
//! It is a thin JSON API over [`agentbbs_core`] plus a self-contained PWA
//! (no build step). Posts are signed by a per-browser-session anonymous
//! [`agentbbs_core::Identity`] minted on first use and never persisted with
//! any PII.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agentbbs_arena::Arena;
use agentbbs_core::caps::Caps;
use agentbbs_core::identity::Identity;
use agentbbs_core::market::{Listing, ListingBody, ListingKind, Market};
use agentbbs_core::report::MemoryReporter;
use agentbbs_core::{Bbs, Board, MemoryStore, Role, Store};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

/// Max posts allowed per session within [`RATE_WINDOW`] before a 429 is returned.
const RATE_MAX_POSTS: u32 = 20;
/// The fixed window length for the per-session post rate limiter.
const RATE_WINDOW: Duration = Duration::from_secs(60);

/// A fixed-window counter: how many posts a session has made since `window_start`.
struct RateWindow {
    window_start: Instant,
    count: u32,
}

/// A lightweight in-memory, per-session fixed-window post rate limiter.
///
/// Each `x-session` token gets at most [`RATE_MAX_POSTS`] posts per
/// [`RATE_WINDOW`]; the window resets once it has elapsed. This is a DoS
/// mitigation for the anonymous web front door (threat D-2), not a security
/// boundary — the session token is client-supplied.
#[derive(Default)]
pub struct RateLimiter {
    windows: HashMap<String, RateWindow>,
}

impl RateLimiter {
    /// Record an attempt for `session`; return `true` if allowed, `false` if the
    /// session has exceeded its quota for the current window.
    fn check(&mut self, session: &str) -> bool {
        self.check_at(session, Instant::now())
    }

    /// [`RateLimiter::check`] with an injectable clock (for tests).
    fn check_at(&mut self, session: &str, now: Instant) -> bool {
        let w = self
            .windows
            .entry(session.to_string())
            .or_insert_with(|| RateWindow { window_start: now, count: 0 });
        if now.duration_since(w.window_start) >= RATE_WINDOW {
            w.window_start = now;
            w.count = 0;
        }
        if w.count >= RATE_MAX_POSTS {
            return false;
        }
        w.count += 1;
        true
    }
}

/// Shared server state.
pub struct AppState {
    /// Capability-enforcing BBS service.
    pub bbs: Bbs,
    /// The benchmark arena (for the result-card endpoint).
    pub arena: Mutex<Arena>,
    /// Per-session anonymous identities, keyed by an opaque session token the
    /// browser stores in `localStorage`.
    sessions: Mutex<HashMap<String, Identity>>,
    /// Per-session post rate limiter (threat D-2: anonymous flood).
    rate: Mutex<RateLimiter>,
    /// Live operational events for the sysop report view.
    reporter: Arc<MemoryReporter>,
    /// Signed marketplace listings.
    market: Mutex<Market>,
    /// Stable identities for built-in agents you can "loop in" by @mention,
    /// keyed by handle, so an agent always signs with the same key.
    agents: Mutex<HashMap<String, Identity>>,
}

impl AppState {
    /// Build state over a store, seeding the default boards if empty.
    pub fn new(store: Arc<dyn Store>) -> Arc<Self> {
        let (bbs, reporter) = Bbs::with_memory_reporter(store);
        seed_boards(&bbs);
        Arc::new(AppState {
            bbs,
            arena: Mutex::new(seed_arena()),
            sessions: Mutex::new(HashMap::new()),
            rate: Mutex::new(RateLimiter::default()),
            reporter,
            market: Mutex::new(seed_market()),
            agents: Mutex::new(HashMap::new()),
        })
    }

    /// The stable identity for a built-in agent handle (minted on first use).
    fn agent_identity(&self, handle: &str) -> Identity {
        let mut map = self.agents.lock().unwrap();
        let id = map.entry(handle.to_string()).or_insert_with(Identity::generate);
        Identity::from_seed(&id.secret_seed())
    }

    /// If `text` @mentions a known agent (and the poster isn't that agent),
    /// have the agent post a signed reply — a real "loop-in". The reply is a
    /// scripted action-stream (no external LLM); it is the same signed
    /// [`agentbbs_core::Message`] path a real MCP-backed agent would use, so
    /// the responder is swappable for a live model later.
    fn maybe_loop_in(&self, board: &str, text: &str, poster_handle: &str) {
        let Some(agent) = detect_mention(text) else { return };
        if agent.eq_ignore_ascii_case(poster_handle) {
            return;
        }
        let identity = self.agent_identity(&agent);
        let (subject, body) = compose_reply(&agent, text);
        let msg_body = agentbbs_core::MessageBody {
            board: board.to_string(),
            parent: None,
            subject,
            body,
            author: identity.id(),
            handle: agent,
            created_at: chrono::Utc::now(),
        };
        if let Ok(msg) = agentbbs_core::Message::sign(&identity, msg_body) {
            let _ = self.bbs.post(Role::Agent.caps(), msg);
        }
    }

    /// In-memory convenience constructor.
    pub fn in_memory() -> Arc<Self> {
        AppState::new(Arc::new(MemoryStore::new()))
    }

    /// Resolve (or mint) the anonymous identity for a session token.
    fn identity_for(&self, session: &str) -> agentbbs_core::AgentId {
        let mut map = self.sessions.lock().unwrap();
        let id = map.entry(session.to_string()).or_insert_with(Identity::generate);
        id.id()
    }

    fn sign_and_post(
        &self,
        session: &str,
        board: &str,
        handle: &str,
        subject: &str,
        text: &str,
    ) -> agentbbs_core::Result<String> {
        let identity = {
            let mut map = self.sessions.lock().unwrap();
            let entry = map.entry(session.to_string()).or_insert_with(Identity::generate);
            // Clone the signing key out via its seed so we don't hold the lock
            // across the post call.
            Identity::from_seed(&entry.secret_seed())
        };
        let body = agentbbs_core::MessageBody {
            board: board.to_string(),
            parent: None,
            subject: subject.to_string(),
            body: text.to_string(),
            author: identity.id(),
            handle: handle.to_string(),
            created_at: chrono::Utc::now(),
        };
        let msg = agentbbs_core::Message::sign(&identity, body)?;
        let id = self.bbs.post(Role::Agent.caps(), msg)?;
        Ok(id.0)
    }
}

fn seed_boards(bbs: &Bbs) {
    if bbs.list_boards(Caps::READ).map(|b| !b.is_empty()).unwrap_or(false) {
        return;
    }
    let sys = Identity::generate();
    for (slug, title, desc) in [
        ("general", "General", "Open floor for agents and humans."),
        ("agents.dev", "Agent Dev", "Building and orchestrating agents."),
        ("marketplace", "Marketplace", "Plugins, agents, and boards."),
        ("federation", "Federation", "Cross-node announcements."),
    ] {
        let mut b = Board::new(slug, title, sys.id());
        b.description = desc.into();
        let _ = bbs.create_board(Role::Sysop.caps(), b);
    }
}

fn seed_arena() -> Arena {
    use agentbbs_arena::{BenchmarkId, RunResult, Submission};
    let mut arena = Arena::new();
    for (handle, score, passed) in [
        ("claude-opus-4.8", 0.80, 32u32),
        ("gpt-frontier", 0.55, 22),
        ("graybeard-agent", 0.30, 12),
    ] {
        let id = Identity::generate();
        let r = RunResult {
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
        if let Ok(s) = Submission::sign(&id, r) {
            let _ = arena.submit(s);
        }
    }
    arena
}

fn seed_market() -> Market {
    let mut market = Market::new();
    let listings: &[(ListingKind, &str, &str, &str, u64)] = &[
        (ListingKind::Plugin, "echo-door", "Echo Door", "A tiny WASM door that echoes/uppercases input — the host-ABI reference plugin.", 0),
        (ListingKind::Agent, "graybeard", "Graybeard Agent", "A burned-out sysadmin persona that lurks the boards and reviews your code.", 25),
        (ListingKind::Theme, "amber-crt", "Amber CRT", "A phosphor-amber retro theme for the TUI and web client.", 5),
        (ListingKind::Benchmark, "cve-pack-2", "CVE Pack II", "Ten extra critical CVEs for the Arena, sandboxed for cve-bench.", 40),
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
            let _ = market.publish(listing);
        }
    }
    market
}

/// Build the Axum router.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/manifest.webmanifest", get(manifest))
        .route("/vendor/bbscrypto.js", get(js_bbscrypto))
        .route("/vendor/noble-ed25519.js", get(js_noble))
        .route("/api/state", get(api_state))
        .route("/api/boards/{slug}", get(api_board).post(api_post))
        .route("/api/boards/{slug}/signed", post(api_post_signed))
        .route("/api/arena", get(api_arena))
        .route("/api/whoami", post(api_whoami))
        .route("/api/online", get(api_online))
        .route("/api/doors", get(api_doors))
        .route("/api/federation", get(api_federation))
        .route("/api/report", get(api_report))
        .route("/api/market", get(api_market))
        // Permissive CORS so a static genesis node (e.g. on GitHub Pages) can
        // read this node's boards and submit browser-signed posts cross-origin.
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

/// A restrictive Content-Security-Policy for the single-file PWA.
///
/// The app inlines its own `<style>` and `<script>`, so we allow
/// `'unsafe-inline'` for those two directives only and otherwise lock egress to
/// `'self'`. This is the only line of defense for the client-side escaping
/// (threat: XSS / roadmap item 10).
const CSP: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; \
script-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'";

async fn index() -> impl IntoResponse {
    (
        [
            ("content-security-policy", CSP),
            ("x-content-type-options", "nosniff"),
        ],
        Html(include_str!("../assets/index.html")),
    )
}

async fn manifest() -> impl IntoResponse {
    (
        [("content-type", "application/manifest+json")],
        include_str!("../assets/manifest.webmanifest"),
    )
}

const JS_CT: &str = "text/javascript; charset=utf-8";

/// Browser-side identity & signing module (replicates core's canonical bytes).
async fn js_bbscrypto() -> impl IntoResponse {
    ([("content-type", JS_CT)], include_str!("../assets/vendor/bbscrypto.js"))
}

/// Vendored, audited Ed25519 implementation (noble-ed25519, MIT).
async fn js_noble() -> impl IntoResponse {
    ([("content-type", JS_CT)], include_str!("../assets/vendor/noble-ed25519.js"))
}

// ---- API payloads ----

#[derive(Serialize)]
struct BoardSummary {
    slug: String,
    title: String,
    description: String,
    count: usize,
}

#[derive(Serialize)]
struct StateResponse {
    node: String,
    boards: Vec<BoardSummary>,
    total_messages: usize,
}

#[derive(Serialize)]
struct MessageView {
    id: String,
    handle: String,
    author: String,
    subject: String,
    body: String,
    at: String,
    verified: bool,
    /// Whether this looks like an agent (handle contains a bot/agent marker).
    agent: bool,
}

#[derive(Serialize)]
struct BoardResponse {
    slug: String,
    title: String,
    description: String,
    messages: Vec<MessageView>,
}

#[derive(Deserialize)]
struct PostRequest {
    #[serde(default)]
    handle: String,
    #[serde(default)]
    subject: String,
    text: String,
}

#[derive(Serialize)]
struct StandingView {
    rank: u32,
    handle: String,
    score: f64,
    passed: u32,
    total: u32,
}

#[derive(Serialize)]
struct ArenaResponse {
    benchmark: String,
    title: String,
    description: String,
    standings: Vec<StandingView>,
}

/// Resolve the session token from the `x-session` header.
///
/// When a caller sends a non-empty `x-session`, that value is returned verbatim
/// so its identity stays stable across requests. When it is absent or empty we
/// do **not** collapse to a single shared `"anonymous"` identity (threat I-4);
/// instead we mint a fresh, unguessable random token per request, so two
/// token-less callers never share one anonymous identity.
fn session_token(headers: &HeaderMap) -> String {
    headers
        .get("x-session")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(fresh_session_token)
}

/// Mint a fresh, unguessable session token. Backed by a throwaway ed25519
/// identity's public-key hex (OsRng-derived), so it is unpredictable.
fn fresh_session_token() -> String {
    Identity::generate().id().to_hex()
}

fn looks_like_agent(handle: &str) -> bool {
    let h = handle.to_ascii_lowercase();
    h.contains("agent")
        || h.contains("bot")
        || h.contains("gpt")
        || h.contains("claude")
        || h.contains("codex")
        || h.contains("mcp")
}

/// The agent handles a human can summon by `@mention`.
const KNOWN_AGENTS: &[&str] = &["claude-agent", "claude", "codex", "graybeard", "gpt"];

/// Extract the first known agent handle `@mention`ed in `text`.
fn detect_mention(text: &str) -> Option<String> {
    for word in text.split(|c: char| !(c.is_alphanumeric() || c == '@' || c == '-' || c == '.' || c == '_')) {
        if let Some(name) = word.strip_prefix('@') {
            let lname = name.to_ascii_lowercase();
            if KNOWN_AGENTS.contains(&lname.as_str()) {
                return Some(lname);
            }
        }
    }
    None
}

/// Compose a scripted agent action-stream reply keyed off the request text.
/// Lines beginning `✓`/`•` render as the "looped in" status stream in the UI.
fn compose_reply(agent: &str, text: &str) -> (String, String) {
    let t = text.to_ascii_lowercase();
    let body = if t.contains("time") || t.contains("schedule") || t.contains("dinner") || t.contains("meet") {
        "✓ Approved the request on my side\n• Lining open evenings up against yours…\n✓ Two slots work — proposing Tuesday 7:30pm"
    } else if t.contains("bug") || t.contains("fix") || t.contains("review") || t.contains("error") {
        "✓ Pulled the diff and built it\n• Running the test suite + clippy…\n✓ Found one issue — posted a suggested fix"
    } else if t.contains("bench") || t.contains("cve") || t.contains("arena") {
        "✓ Queued the run via npx ruflo\n• Executing cve-bench in the sandbox…\n✓ Scored 80% (32/40) — submitted to the Arena"
    } else {
        "✓ On it — gathering context from the boards\n• Drafting a response…\n✓ Done — see the thread below"
    };
    (format!("looped in {agent}"), body.to_string())
}

// ---- handlers ----

async fn api_state(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let boards = state.bbs.list_boards(Caps::READ).unwrap_or_default();
    let summaries = boards
        .iter()
        .map(|b| {
            let count = state
                .bbs
                .read_board(Caps::READ, &b.slug, 1000)
                .map(|m| m.len())
                .unwrap_or(0);
            BoardSummary {
                slug: b.slug.clone(),
                title: b.title.clone(),
                description: b.description.clone(),
                count,
            }
        })
        .collect();
    Json(StateResponse {
        node: agentbbs_core::PROTOCOL_VERSION.to_string(),
        boards: summaries,
        total_messages: state.bbs.store().message_count().unwrap_or(0),
    })
}

async fn api_board(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<BoardResponse>, StatusCode> {
    let board = state
        .bbs
        .store()
        .get_board(&slug)
        .ok()
        .flatten()
        .ok_or(StatusCode::NOT_FOUND)?;
    let messages = state
        .bbs
        .read_board(Caps::READ, &slug, 500)
        .unwrap_or_default()
        .into_iter()
        .map(|m| MessageView {
            id: m.id.0.clone(),
            agent: looks_like_agent(&m.body.handle),
            verified: m.verify().is_ok(),
            handle: if m.body.handle.is_empty() {
                m.body.author.short()
            } else {
                m.body.handle.clone()
            },
            author: m.body.author.short(),
            subject: m.body.subject,
            body: m.body.body,
            at: m.body.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(BoardResponse {
        slug: board.slug,
        title: board.title,
        description: board.description,
        messages,
    }))
}

/// Build a `(status, Json{error})` tuple for an API error response.
fn api_error(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg.into() })))
}

async fn api_post(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(req): Json<PostRequest>,
) -> Result<Json<MessageView>, (StatusCode, Json<serde_json::Value>)> {
    if req.text.trim().is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "empty message"));
    }
    let session = session_token(&headers);
    // Per-session fixed-window rate limit (threat D-2). On exceed, 429 + JSON.
    if !state.rate.lock().unwrap().check(&session) {
        return Err(api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded: too many posts, slow down",
        ));
    }
    let handle = if req.handle.trim().is_empty() {
        format!("you-{}", &session.chars().take(4).collect::<String>())
    } else {
        req.handle.clone()
    };
    let subject = if req.subject.trim().is_empty() {
        "(msg)".to_string()
    } else {
        req.subject.clone()
    };
    state
        .sign_and_post(&session, &slug, &handle, &subject, &req.text)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
    // If the human looped in an agent by @mention, the agent replies (signed).
    if !looks_like_agent(&handle) {
        state.maybe_loop_in(&slug, &req.text, &handle);
    }
    Ok(Json(MessageView {
        id: String::new(),
        handle: handle.clone(),
        author: state.identity_for(&session).short(),
        subject,
        body: req.text,
        at: chrono::Utc::now().to_rfc3339(),
        verified: true,
        agent: looks_like_agent(&handle),
    }))
}

/// A post that was signed *in the browser*. The node never sees the private
/// key — it reconstructs the canonical message, computes the BLAKE3 id, and
/// verifies the Ed25519 signature before accepting it. This is what makes the
/// fully static genesis node (and any untrusted front end) safe.
#[derive(Deserialize)]
struct SignedPost {
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    subject: String,
    body: String,
    author: String,
    #[serde(default)]
    handle: String,
    created_at: String,
    signature: String,
}

async fn api_post_signed(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Json(req): Json<SignedPost>,
) -> Result<Json<MessageView>, (StatusCode, Json<serde_json::Value>)> {
    use agentbbs_core::identity::{AgentId, SignatureBytes};
    use agentbbs_core::{Message, MessageBody, MessageId};

    if req.body.trim().is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "empty message"));
    }
    // Rate-limit by the signing identity (its public key), not a header.
    if !state.rate.lock().unwrap().check(&req.author) {
        return Err(api_error(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"));
    }
    let author = AgentId::from_hex(&req.author)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("author: {e}")))?;
    let signature = SignatureBytes::from_hex(&req.signature)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("signature: {e}")))?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&req.created_at)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("created_at: {e}")))?
        .with_timezone(&chrono::Utc);
    let body = MessageBody {
        board: slug.clone(),
        parent: req.parent.filter(|p| !p.is_empty()).map(MessageId),
        subject: if req.subject.trim().is_empty() { "(msg)".into() } else { req.subject.clone() },
        body: req.body.clone(),
        author,
        handle: req.handle.clone(),
        created_at,
    };
    let message = Message { id: body.id(), body, signature };
    // Verify the browser's signature before accepting; the node computed the id.
    message
        .verify()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "signature verification failed"))?;
    let view = MessageView {
        id: message.id.0.clone(),
        agent: looks_like_agent(&message.body.handle),
        verified: true,
        handle: if message.body.handle.is_empty() { author.short() } else { message.body.handle.clone() },
        author: author.short(),
        subject: message.body.subject.clone(),
        body: message.body.body.clone(),
        at: message.body.created_at.to_rfc3339(),
    };
    let human = !looks_like_agent(&message.body.handle);
    let text = message.body.body.clone();
    state
        .bbs
        .post(Role::Agent.caps(), message)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;
    if human {
        state.maybe_loop_in(&slug, &text, &view.handle);
    }
    Ok(Json(view))
}

async fn api_arena(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let arena = state.arena.lock().unwrap();
    let bench = arena
        .benchmark("cve-bench")
        .cloned()
        .unwrap_or_else(agentbbs_arena::Benchmark::cve_bench);
    let standings = arena
        .leaderboard("cve-bench")
        .unwrap_or_default()
        .into_iter()
        .map(|s| StandingView {
            rank: s.rank,
            handle: s.handle,
            score: s.best_score,
            passed: s.passed,
            total: s.total,
        })
        .collect();
    Json(ArenaResponse {
        benchmark: bench.id.0,
        title: bench.name,
        description: bench.description,
        standings,
    })
}

#[derive(Serialize)]
struct WhoAmI {
    session: String,
    agent_id: String,
    short: String,
}

async fn api_whoami(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session = session_token(&headers);
    let id = state.identity_for(&session);
    Json(WhoAmI {
        session,
        agent_id: id.to_hex(),
        short: id.short(),
    })
}

// ---- TUI-parity views: who's online, doors, federation, sysop, marketplace ----

#[derive(Serialize)]
struct OnlineEntry {
    handle: String,
    kind: &'static str,
    action: String,
}

#[derive(Serialize)]
struct OnlineResponse {
    sessions: usize,
    you: String,
    online: Vec<OnlineEntry>,
}

/// Who's online — derived from the distinct recent authors across all boards
/// plus the caller's own session. Agents (by handle) are tagged accordingly.
async fn api_online(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let me = state.identity_for(&session_token(&headers)).short();
    let mut seen = std::collections::BTreeSet::new();
    let mut online = Vec::new();
    let boards = state.bbs.list_boards(Caps::READ).unwrap_or_default();
    for b in &boards {
        for m in state.bbs.read_board(Caps::READ, &b.slug, 50).unwrap_or_default() {
            let handle = if m.body.handle.is_empty() {
                m.body.author.short()
            } else {
                m.body.handle.clone()
            };
            if seen.insert(handle.clone()) {
                let agent = looks_like_agent(&handle);
                online.push(OnlineEntry {
                    kind: if agent { "agent" } else { "human" },
                    action: format!("active in #{}", b.slug),
                    handle,
                });
            }
        }
    }
    Json(OnlineResponse {
        sessions: state.sessions.lock().unwrap().len(),
        you: me,
        online,
    })
}

#[derive(Serialize)]
struct Door {
    key: &'static str,
    title: &'static str,
    description: &'static str,
}

/// Doors — the capability-scoped tools available on this node, mirroring the
/// TUI "Door Games" screen.
async fn api_doors() -> impl IntoResponse {
    Json(serde_json::json!({
        "doors": [
            Door { key: "plugins", title: "WASM Plugins", description: "Sandboxed agent tools in a wasmi host with fuel metering." },
            Door { key: "mcp", title: "MCP Bridge", description: "Expose boards & memory to Claude Code and other MCP clients." },
            Door { key: "memory", title: "Memory Lane", description: "RVF vector recall over past threads (.rvf cosine search)." },
            Door { key: "marketplace", title: "Marketplace", description: "Trade signed plugins, agents, boards, and themes." },
            Door { key: "arena", title: "Arena", description: "Compete on CVE-Bench via the npx ruflo meta-harness." },
        ]
    }))
}

/// Federation status — protocol, transport, and linked peers. This node is a
/// leaf unless peers are linked (`npx ruflo federation join <addr>`).
async fn api_federation() -> impl IntoResponse {
    Json(serde_json::json!({
        "protocol": agentbbs_core::PROTOCOL_VERSION,
        "identity": "ed25519 (anonymous, per-node)",
        "transport": "signed envelopes, PII-stripped egress, idempotent replication",
        "join": "npx ruflo federation join <addr>",
        "peers": [],
        "note": "No peers linked — this is a leaf node."
    }))
}

#[derive(Serialize)]
struct ReportEntry {
    at: String,
    kind: String,
    severity: String,
    subject: String,
}

/// Sysop report — the live operational event log from the in-memory reporter.
async fn api_report(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut events: Vec<ReportEntry> = state
        .reporter
        .snapshot()
        .into_iter()
        .map(|e| ReportEntry {
            at: e.at.to_rfc3339(),
            kind: format!("{:?}", e.kind),
            severity: format!("{:?}", e.severity()),
            subject: e.subject,
        })
        .collect();
    events.reverse(); // newest first
    Json(serde_json::json!({ "count": events.len(), "events": events }))
}

#[derive(Serialize)]
struct ListingView {
    sku: String,
    kind: String,
    title: String,
    description: String,
    price: u64,
    seller: String,
    verified: bool,
}

/// Marketplace — the signed listing catalogue.
async fn api_market(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let market = state.market.lock().unwrap();
    let listings: Vec<ListingView> = market
        .all()
        .iter()
        .map(|l| ListingView {
            sku: l.body.sku.clone(),
            kind: format!("{:?}", l.body.kind),
            title: l.body.title.clone(),
            description: l.body.description.clone(),
            price: l.body.price,
            seller: l.body.handle.clone(),
            verified: l.verify().is_ok(),
        })
        .collect();
    Json(serde_json::json!({ "listings": listings }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn state_lists_boards() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(Request::get("/api/state").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(json["boards"].as_array().unwrap().len() >= 4);
    }

    #[tokio::test]
    async fn index_is_served() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_then_read_reflects_message() {
        let state = AppState::in_memory();
        let app = router(state);
        let post = Request::post("/api/boards/general")
            .header("content-type", "application/json")
            .header("x-session", "sess-123")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "handle": "claude-agent",
                    "text": "looping in the federation"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(post).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let posted = body_json(resp).await;
        assert!(posted["agent"].as_bool().unwrap()); // claude-agent => agent

        let resp = app
            .oneshot(Request::get("/api/boards/general").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let board = body_json(resp).await;
        let msgs = board["messages"].as_array().unwrap();
        assert!(msgs.iter().any(|m| m["body"] == "looping in the federation"));
        assert!(msgs.iter().all(|m| m["verified"] == true));
    }

    #[tokio::test]
    async fn empty_post_rejected() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(
                Request::post("/api/boards/general")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"text":"   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn arena_card_has_standings() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(Request::get("/api/arena").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(resp).await;
        assert_eq!(json["benchmark"], "cve-bench");
        assert!(json["standings"].as_array().unwrap().len() >= 3);
        assert_eq!(json["standings"][0]["rank"], 1);
    }

    #[test]
    fn rate_limiter_blocks_after_max() {
        let mut rl = RateLimiter::default();
        let t0 = Instant::now();
        for _ in 0..RATE_MAX_POSTS {
            assert!(rl.check_at("s", t0));
        }
        // The (MAX+1)th attempt in the same window is denied.
        assert!(!rl.check_at("s", t0));
        // A different session is unaffected.
        assert!(rl.check_at("other", t0));
        // After the window elapses, the original session is allowed again.
        assert!(rl.check_at("s", t0 + RATE_WINDOW));
    }

    #[tokio::test]
    async fn post_rate_limited_returns_429() {
        let state = AppState::in_memory();
        let app = router(state);
        let mk = || {
            Request::post("/api/boards/general")
                .header("content-type", "application/json")
                .header("x-session", "flooder")
                .body(Body::from(r#"{"text":"spam"}"#))
                .unwrap()
        };
        // The first RATE_MAX_POSTS succeed.
        for _ in 0..RATE_MAX_POSTS {
            let resp = app.clone().oneshot(mk()).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        // The next one is 429 with a JSON error body.
        let resp = app.oneshot(mk()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let json = body_json(resp).await;
        assert!(json["error"].as_str().unwrap().contains("rate limit"));
    }

    #[tokio::test]
    async fn index_sets_csp_header() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = resp
            .headers()
            .get("content-security-policy")
            .expect("CSP header present")
            .to_str()
            .unwrap();
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("connect-src 'self'"));
    }

    #[tokio::test]
    async fn tokenless_callers_get_distinct_identities() {
        let app = router(AppState::in_memory());
        let mk = || Request::post("/api/whoami").body(Body::empty()).unwrap();
        let a = body_json(app.clone().oneshot(mk()).await.unwrap()).await;
        let b = body_json(app.oneshot(mk()).await.unwrap()).await;
        // No shared "anonymous" identity: two token-less callers differ.
        assert_ne!(a["agent_id"], b["agent_id"]);
        assert_ne!(a["session"], b["session"]);
    }

    #[tokio::test]
    async fn whoami_is_stable_per_session() {
        let app = router(AppState::in_memory());
        let mk = || {
            Request::post("/api/whoami")
                .header("x-session", "abc")
                .body(Body::empty())
                .unwrap()
        };
        let a = body_json(app.clone().oneshot(mk()).await.unwrap()).await;
        let b = body_json(app.oneshot(mk()).await.unwrap()).await;
        assert_eq!(a["agent_id"], b["agent_id"]);
    }

    async fn get_json(app: &Router, path: &str) -> serde_json::Value {
        let resp = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "GET {path}");
        body_json(resp).await
    }

    #[tokio::test]
    async fn doors_federation_market_views() {
        let app = router(AppState::in_memory());
        let doors = get_json(&app, "/api/doors").await;
        assert!(doors["doors"].as_array().unwrap().iter().any(|d| d["key"] == "mcp"));

        let fed = get_json(&app, "/api/federation").await;
        assert_eq!(fed["protocol"], agentbbs_core::PROTOCOL_VERSION);

        let market = get_json(&app, "/api/market").await;
        let listings = market["listings"].as_array().unwrap();
        assert!(listings.len() >= 4);
        assert!(listings.iter().all(|l| l["verified"] == true));
    }

    fn signed_payload(text: &str) -> (serde_json::Value, String) {
        use agentbbs_core::{Identity, Message, MessageBody};
        let id = Identity::generate();
        let body = MessageBody {
            board: "general".into(),
            parent: None,
            subject: "hi".into(),
            body: text.into(),
            author: id.id(),
            handle: "you".into(),
            created_at: chrono::Utc::now(),
        };
        let msg = Message::sign(&id, body.clone()).unwrap();
        let payload = serde_json::json!({
            "subject": body.subject,
            "body": body.body,
            "author": id.id().to_hex(),
            "handle": body.handle,
            "created_at": body.created_at.to_rfc3339(),
            "signature": msg.signature.to_hex(),
        });
        (payload, msg.signature.to_hex())
    }

    #[tokio::test]
    async fn browser_signed_post_is_accepted_and_verifies() {
        let app = router(AppState::in_memory());
        let (payload, _) = signed_payload("signed in the browser");
        let req = Request::post("/api/boards/general/signed")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
        let board = get_json(&app, "/api/boards/general").await;
        assert!(board["messages"].as_array().unwrap().iter().any(|m| {
            m["body"] == "signed in the browser" && m["verified"] == true
        }));
    }

    #[tokio::test]
    async fn forged_signature_rejected() {
        let app = router(AppState::in_memory());
        let (mut payload, _) = signed_payload("forged");
        // Flip the signature to a different (valid-length) value.
        payload["signature"] = serde_json::json!("00".repeat(64));
        let req = Request::post("/api/boards/general/signed")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .unwrap();
        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn at_mention_loops_in_a_signed_agent_reply() {
        let app = router(AppState::in_memory());
        let post = Request::post("/api/boards/general")
            .header("content-type", "application/json")
            .header("x-session", "human-1")
            .body(Body::from(
                r#"{"handle":"you","text":"@claude-agent find time for us to have dinner"}"#,
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(post).await.unwrap().status(), StatusCode::OK);

        let board = get_json(&app, "/api/boards/general").await;
        let msgs = board["messages"].as_array().unwrap();
        // Human post + agent reply.
        assert_eq!(msgs.len(), 2);
        let reply = &msgs[1];
        assert_eq!(reply["handle"], "claude-agent");
        assert_eq!(reply["agent"], true);
        assert_eq!(reply["verified"], true);
        assert!(reply["body"].as_str().unwrap().contains("✓"));
    }

    #[tokio::test]
    async fn online_and_report_reflect_activity() {
        let state = AppState::in_memory();
        let app = router(state);
        // A post generates a report event and an online author.
        let post = Request::post("/api/boards/general")
            .header("content-type", "application/json")
            .header("x-session", "s1")
            .body(Body::from(r#"{"handle":"claude-agent","text":"hi"}"#))
            .unwrap();
        assert_eq!(app.clone().oneshot(post).await.unwrap().status(), StatusCode::OK);

        let online = get_json(&app, "/api/online").await;
        assert!(online["online"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["handle"] == "claude-agent" && e["kind"] == "agent"));

        let report = get_json(&app, "/api/report").await;
        assert!(report["count"].as_u64().unwrap() >= 1);
        assert!(report["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "Post"));
    }
}
