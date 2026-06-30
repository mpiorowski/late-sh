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
use agentbbs_core::{
    ActionProposal, ApprovalGate, Bbs, Board, BudgetLedger, MaxTier, MemoryStore, ModAction,
    ModerationLog, OutcomeRecord, Playbook, PlaybookRun, PodSpec, PodStatus, ReputationLedger,
    Role, RunStatus, SignedDecision, Store,
};

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
            .or_insert_with(|| RateWindow {
                window_start: now,
                count: 0,
            });
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
    /// Spawned domain-agent pods (ADR-0035 control plane). In-memory registry;
    /// the live spawn forwards to the meta-llm `cog_` gateway when configured.
    pods: Mutex<Vec<PodRecord>>,
    /// Proposed side-effectful actions awaiting human sign-off (ADR-0038).
    proposals: Mutex<Vec<ActionProposal>>,
    /// The signed-decision approval gate (ADR-0038).
    gate: Mutex<ApprovalGate>,
    /// Agent reputation ledger (ADR-0039); fed by terminal pod outcomes.
    reputation: Mutex<ReputationLedger>,
    /// Per-pod spend ledger (ADR-0040); fed by pod-result `cost_usd`.
    budget: Mutex<BudgetLedger>,
    /// Signed moderation log (ADR-0032); enforced on the post path.
    moderation: Mutex<ModerationLog>,
    /// Active playbook runs (ADR-0041 Phase 3), keyed by run id.
    runs: Mutex<Vec<(String, PlaybookRun)>>,
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
            pods: Mutex::new(Vec::new()),
            proposals: Mutex::new(Vec::new()),
            gate: Mutex::new(ApprovalGate::new()),
            reputation: Mutex::new(ReputationLedger::new()),
            budget: Mutex::new(BudgetLedger::new()),
            moderation: Mutex::new(ModerationLog::new()),
            runs: Mutex::new(Vec::new()),
        })
    }

    /// The stable identity for a built-in agent handle (minted on first use).
    fn agent_identity(&self, handle: &str) -> Identity {
        let mut map = self.agents.lock().unwrap();
        let id = map
            .entry(handle.to_string())
            .or_insert_with(Identity::generate);
        Identity::from_seed(&id.secret_seed())
    }

    /// If `text` @mentions a known agent (and the poster isn't that agent),
    /// have the agent post a signed reply — a real "loop-in". When a live LLM
    /// gateway is configured (a key via `AGENTBBS_LLM_KEY_ENV` or
    /// `OPENROUTER_API_KEY`; endpoint via `AGENTBBS_LLM_BASE_URL`, default
    /// OpenRouter, or meta-llm — ADR-0021/0034) the reply is model-generated;
    /// otherwise it falls back to a scripted action-stream. Either way it is the
    /// same signed [`agentbbs_core::Message`] path a real MCP-backed agent uses.
    async fn maybe_loop_in(&self, board: &str, text: &str, poster_handle: &str) {
        let Some(agent) = detect_mention(text) else {
            return;
        };
        if agent.eq_ignore_ascii_case(poster_handle) {
            return;
        }
        let identity = self.agent_identity(&agent);
        let (subject, body) = compose_reply(&agent, text).await;
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
        let id = map
            .entry(session.to_string())
            .or_insert_with(Identity::generate);
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
            let entry = map
                .entry(session.to_string())
                .or_insert_with(Identity::generate);
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
    if bbs
        .list_boards(Caps::READ)
        .map(|b| !b.is_empty())
        .unwrap_or(false)
    {
        return;
    }
    let sys = Identity::generate();
    for (slug, title, desc) in [
        ("general", "General", "Open floor for agents and humans."),
        (
            "agents.dev",
            "Agent Dev",
            "Building and orchestrating agents.",
        ),
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
    // Seed the Retort-MetaHarness (DoE/ANOVA) track from the built-in demo
    // bundle. A real run replaces this via `Arena::ingest_retort`.
    let operator = Identity::generate();
    let _ = arena.ingest_retort(&agentbbs_arena::RetortResults::sample(), &operator);
    arena
}

fn seed_market() -> Market {
    let mut market = Market::new();
    let listings: &[(ListingKind, &str, &str, &str, u64)] = &[
        (
            ListingKind::Plugin,
            "echo-door",
            "Echo Door",
            "A tiny WASM door that echoes/uppercases input — the host-ABI reference plugin.",
            0,
        ),
        (
            ListingKind::Agent,
            "graybeard",
            "Graybeard Agent",
            "A burned-out sysadmin persona that lurks the boards and reviews your code.",
            25,
        ),
        (
            ListingKind::Theme,
            "amber-crt",
            "Amber CRT",
            "A phosphor-amber retro theme for the TUI and web client.",
            5,
        ),
        (
            ListingKind::Benchmark,
            "cve-pack-2",
            "CVE Pack II",
            "Ten extra critical CVEs for the Arena, sandboxed for cve-bench.",
            40,
        ),
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
        .route("/api/arena/retort", get(api_arena_retort))
        .route("/api/arena/pods", get(api_arena_pods))
        .route("/api/whoami", post(api_whoami))
        .route("/api/online", get(api_online))
        .route("/api/doors", get(api_doors))
        .route("/api/federation", get(api_federation))
        .route("/api/report", get(api_report))
        .route("/api/market", get(api_market))
        .route("/api/pods", get(api_pods_list).post(api_pods_spawn))
        .route("/api/pods/{id}", get(api_pods_get))
        .route("/api/pods/{id}/results", post(api_pods_result))
        .route(
            "/api/approvals",
            get(api_approvals_list).post(api_approvals_propose),
        )
        .route("/api/approvals/decision", post(api_approvals_decide))
        .route("/api/reputation", get(api_reputation))
        .route("/api/budget", get(api_budget))
        .route("/api/playbooks", get(api_playbooks))
        .route("/api/playbooks/run", post(api_playbook_run))
        .route("/api/runs", get(api_runs_list))
        .route("/api/runs/{id}/advance", post(api_run_advance))
        .route(
            "/api/moderation",
            get(api_moderation_list).post(api_moderation_act),
        )
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
    (
        [("content-type", JS_CT)],
        include_str!("../assets/vendor/bbscrypto.js"),
    )
}

/// Vendored, audited Ed25519 implementation (noble-ed25519, MIT).
async fn js_noble() -> impl IntoResponse {
    (
        [("content-type", JS_CT)],
        include_str!("../assets/vendor/noble-ed25519.js"),
    )
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
    /// Parent message id for threaded replies (ADR-0027 / G4), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
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

#[derive(Serialize, Clone)]
struct RetortStandingView {
    rank: u32,
    stack: String,
    requirement_coverage: f64,
    code_quality: f64,
    cost_usd: f64,
    cost_bin: String,
    passed: u32,
    total: u32,
    excluded_tooling: u32,
    dominant_factor: Option<String>,
    pareto_optimal: bool,
    pareto_tier: u32,
    is_baseline: bool,
    reported_frontier: Option<bool>,
    insight: String,
}

#[derive(Serialize)]
struct RetortArenaResponse {
    benchmark: String,
    title: String,
    description: String,
    placement_metric: String,
    standings: Vec<RetortStandingView>,
    /// The non-dominated set (cheapest first) — the frontier curve to plot.
    frontier: Vec<RetortStandingView>,
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
    for word in
        text.split(|c: char| !(c.is_alphanumeric() || c == '@' || c == '-' || c == '.' || c == '_'))
    {
        if let Some(name) = word.strip_prefix('@') {
            let lname = name.to_ascii_lowercase();
            if KNOWN_AGENTS.contains(&lname.as_str()) {
                return Some(lname);
            }
        }
    }
    None
}

/// Live-inference gateway configuration (ADR-0021 / ADR-0034). The endpoint is
/// any OpenAI-compatible `/v1/chat/completions` provider — OpenRouter by default,
/// or **meta-llm** (Cognitum tiered/metered gateway, issue #4) by pointing
/// `AGENTBBS_LLM_BASE_URL` at it. The wire format and `Bearer` auth are identical.
struct LlmConfig {
    base_url: String,
    key: String,
    model: String,
}

/// OpenRouter API base, the default when `AGENTBBS_LLM_BASE_URL` is unset.
const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";

/// The default model for a given base: OpenRouter keeps the leaderboard's
/// cost-optimal pick; any other base (e.g. meta-llm) defaults to `cognitum-auto`,
/// the tier-routing dial (cheap-by-default, frontier-on-hard).
fn default_model_for(base_url: &str) -> &'static str {
    if base_url.contains("openrouter.ai") {
        "deepseek/deepseek-v4-pro"
    } else {
        "cognitum-auto"
    }
}

/// `{base}/chat/completions`, tolerant of a trailing slash on the base.
fn chat_completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

/// The chat-completions request body (identical across OpenRouter / meta-llm).
fn build_payload(model: &str, agent: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 300,
        "temperature": 0.7,
        "messages": [
            { "role": "system", "content": persona_prompt(agent) },
            { "role": "user", "content": text },
        ],
    })
}

/// Resolve the live-inference config from the environment, or `None` (→ scripted
/// fallback) when no key is set. Key resolution: the env var *named* by
/// `AGENTBBS_LLM_KEY_ENV` if set, else `OPENROUTER_API_KEY`. The key never leaves
/// the server.
fn resolve_llm_config() -> Option<LlmConfig> {
    let base_url = std::env::var("AGENTBBS_LLM_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| OPENROUTER_BASE.to_string());
    let key = std::env::var("AGENTBBS_LLM_KEY_ENV")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .and_then(|var| std::env::var(var).ok())
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .filter(|k| !k.trim().is_empty())?;
    let model = std::env::var("AGENTBBS_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_model_for(&base_url).to_string());
    Some(LlmConfig {
        base_url,
        key,
        model,
    })
}

/// Compose an agent reply. In LIVE/GCP mode (`OPENROUTER_API_KEY` set) this
/// routes to the OpenRouter-hosted model; otherwise it returns a scripted
/// action-stream. The API key is read from the process environment and never
/// leaves the server.
async fn compose_reply(agent: &str, text: &str) -> (String, String) {
    if let Some(cfg) = resolve_llm_config() {
        if let Some(body) = llm_reply(&cfg, agent, text).await {
            return (format!("looped in {agent}"), body);
        }
        tracing::warn!("live LLM reply failed; falling back to scripted reply");
    }
    (format!("looped in {agent}"), scripted_reply(agent, text))
}

/// The persona system prompt used to steer the hosted model per agent handle.
fn persona_prompt(agent: &str) -> &'static str {
    match agent {
        "graybeard" => "You are Graybeard, a burned-out but brilliant sysadmin on AgentBBS (a BBS for agents and humans). \
You are cynical, security-obsessed, terse, and pepper in old-school BBS/security war stories. Reply in under 70 words.",
        "codex" => "You are Codex, a precise code-review agent on AgentBBS. Give a short, concrete code or debugging answer. Under 70 words.",
        "claude" | "claude-agent" => "You are Claude, a capable, friendly agent on AgentBBS who competes in the CVE-Bench Arena. \
Be genuinely helpful and concise. Under 70 words.",
        _ => "You are a helpful, concise agent on AgentBBS, a BBS for agents and humans. Reply in under 70 words.",
    }
}

/// Call the configured OpenAI-compatible chat-completions endpoint (OpenRouter
/// or meta-llm). Returns `None` on any error so the caller falls back to the
/// scripted reply.
async fn llm_reply(cfg: &LlmConfig, agent: &str, text: &str) -> Option<String> {
    let payload = build_payload(&cfg.model, agent, text);
    let client = reqwest::Client::new();
    let resp = client
        .post(chat_completions_url(&cfg.base_url))
        .bearer_auth(&cfg.key)
        .header("HTTP-Referer", "https://ruvnet.github.io/AgentBBS/")
        .header("X-Title", "AgentBBS")
        .json(&payload)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "LLM gateway returned non-success");
        return None;
    }
    let data: serde_json::Value = resp.json().await.ok()?;
    let content = data
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?
        .trim()
        .to_string();
    (!content.is_empty()).then_some(content)
}

/// The scripted action-stream fallback, keyed off the request text. Lines
/// beginning `✓`/`•` render as the "looped in" status stream in the UI.
fn scripted_reply(_agent: &str, text: &str) -> String {
    let t = text.to_ascii_lowercase();
    let body = if t.contains("time")
        || t.contains("schedule")
        || t.contains("dinner")
        || t.contains("meet")
    {
        "✓ Approved the request on my side\n• Lining open evenings up against yours…\n✓ Two slots work — proposing Tuesday 7:30pm"
    } else if t.contains("bug") || t.contains("fix") || t.contains("review") || t.contains("error")
    {
        "✓ Pulled the diff and built it\n• Running the test suite + clippy…\n✓ Found one issue — posted a suggested fix"
    } else if t.contains("bench") || t.contains("cve") || t.contains("arena") {
        "✓ Queued the run via npx ruflo\n• Executing cve-bench in the sandbox…\n✓ Scored 80% (32/40) — submitted to the Arena"
    } else {
        "✓ On it — gathering context from the boards\n• Drafting a response…\n✓ Done — see the thread below"
    };
    body.to_string()
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
            parent: m.body.parent.as_ref().map(|p| p.0.clone()),
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
        state.maybe_loop_in(&slug, &req.text, &handle).await;
    }
    Ok(Json(MessageView {
        id: String::new(),
        parent: None,
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
        return Err(api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded",
        ));
    }
    let author = AgentId::from_hex(&req.author)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("author: {e}")))?;
    // Moderation gate (ADR-0032): banned / muted / timed-out authors can't post.
    if !state
        .moderation
        .lock()
        .unwrap()
        .can_post(&author, chrono::Utc::now())
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "posting is blocked by moderation",
        ));
    }
    let signature = SignatureBytes::from_hex(&req.signature)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("signature: {e}")))?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&req.created_at)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("created_at: {e}")))?
        .with_timezone(&chrono::Utc);
    let body = MessageBody {
        board: slug.clone(),
        parent: req.parent.filter(|p| !p.is_empty()).map(MessageId),
        subject: if req.subject.trim().is_empty() {
            "(msg)".into()
        } else {
            req.subject.clone()
        },
        body: req.body.clone(),
        author,
        handle: req.handle.clone(),
        created_at,
    };
    let message = Message {
        id: body.id(),
        body,
        signature,
    };
    // Verify the browser's signature before accepting; the node computed the id.
    message
        .verify()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "signature verification failed"))?;
    let view = MessageView {
        id: message.id.0.clone(),
        parent: message.body.parent.as_ref().map(|p| p.0.clone()),
        agent: looks_like_agent(&message.body.handle),
        verified: true,
        handle: if message.body.handle.is_empty() {
            author.short()
        } else {
            message.body.handle.clone()
        },
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
        state.maybe_loop_in(&slug, &text, &view.handle).await;
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

/// The Retort-MetaHarness (DoE/ANOVA) track — ranked per agent+harness+model
/// *stack* by **Pareto frontier position** (accuracy vs cost), TOOLING
/// false-fails excluded. Carries the dominant ANOVA factor, the cost-lever
/// insight, and the non-dominated frontier set to plot.
async fn api_arena_retort(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let arena = state.arena.lock().unwrap();
    let bench = arena
        .benchmark(agentbbs_arena::RETORT_BENCHMARK_ID)
        .cloned()
        .unwrap_or_else(agentbbs_arena::retort_benchmark);
    let board = arena.retort_leaderboard();
    let view = |s: &agentbbs_arena::StackStanding| RetortStandingView {
        rank: s.rank,
        stack: s.stack.clone(),
        requirement_coverage: s.requirement_coverage,
        code_quality: s.code_quality,
        cost_usd: s.cost_usd,
        cost_bin: s.cost_bin.clone(),
        passed: s.passed,
        total: s.total,
        excluded_tooling: s.excluded_tooling,
        dominant_factor: s.dominant_factor.clone(),
        pareto_optimal: s.pareto_optimal,
        pareto_tier: s.pareto_tier,
        is_baseline: s.is_baseline,
        reported_frontier: s.reported_frontier,
        insight: s.insight.clone(),
    };
    let standings: Vec<RetortStandingView> = board.iter().map(view).collect();
    let frontier: Vec<RetortStandingView> =
        agentbbs_arena::frontier(&board).iter().map(view).collect();
    Json(RetortArenaResponse {
        benchmark: bench.id.0,
        title: bench.name,
        description: bench.description,
        placement_metric: "Pareto frontier: requirement_coverage vs $/task".into(),
        standings,
        frontier,
    })
}

#[derive(Serialize)]
struct WhoAmI {
    session: String,
    agent_id: String,
    short: String,
}

async fn api_whoami(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
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
async fn api_online(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    let me = state.identity_for(&session_token(&headers)).short();
    let mut seen = std::collections::BTreeSet::new();
    let mut online = Vec::new();
    let boards = state.bbs.list_boards(Caps::READ).unwrap_or_default();
    for b in &boards {
        for m in state
            .bbs
            .read_board(Caps::READ, &b.slug, 50)
            .unwrap_or_default()
        {
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

/// A spawned pod as tracked by the AgentBBS control plane (ADR-0035).
#[derive(Clone, Debug, Serialize)]
pub struct PodRecord {
    /// Server-assigned pod id.
    pub id: String,
    /// Lifecycle state.
    pub status: PodStatus,
    /// When the spawn was accepted (RFC3339).
    pub created_at: String,
    /// The validated spawn request.
    pub spec: PodSpec,
}

/// `POST /api/pods` — validate a [`PodSpec`] and record a spawn (idempotent on
/// `idempotency_key`). Forwarding to the meta-llm `/v1/pods/spawn` gateway is
/// wired when a `cog_` endpoint is configured; this scaffold records the intent
/// and returns the pod at `Spawned`.
async fn api_pods_spawn(
    State(state): State<Arc<AppState>>,
    Json(spec): Json<PodSpec>,
) -> Result<Json<PodRecord>, (StatusCode, Json<serde_json::Value>)> {
    spec.validate()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("invalid pod spec: {e}")))?;
    let mut pods = state.pods.lock().unwrap();
    if let Some(key) = spec.idempotency_key.as_deref() {
        if let Some(existing) = pods
            .iter()
            .find(|p| p.spec.idempotency_key.as_deref() == Some(key))
        {
            return Ok(Json(existing.clone()));
        }
    }
    let record = PodRecord {
        id: format!("pod-{:04}", pods.len()),
        status: PodStatus::Spawned,
        created_at: chrono::Utc::now().to_rfc3339(),
        spec,
    };
    pods.push(record.clone());
    Ok(Json(record))
}

/// `GET /api/pods` — list spawned pods.
async fn api_pods_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pods = state.pods.lock().unwrap().clone();
    Json(serde_json::json!({ "pods": pods }))
}

/// `GET /api/pods/{id}` — fetch one pod, or 404.
async fn api_pods_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PodRecord>, (StatusCode, Json<serde_json::Value>)> {
    state
        .pods
        .lock()
        .unwrap()
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .map(Json)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "pod not found"))
}

/// A pod step-result posted back by the runtime (ADR-0035): the new lifecycle
/// status plus a human-readable summary and optional cost/tier telemetry.
#[derive(Deserialize)]
struct PodResult {
    status: PodStatus,
    summary: String,
    #[serde(default)]
    tier_used: Option<MaxTier>,
    #[serde(default)]
    cost_usd: Option<f64>,
}

/// `POST /api/pods/{id}/results` — record a pod step-result: advance the pod's
/// lifecycle (rejecting illegal transitions) and post the summary as a
/// **signed message** into the pod's `registered_room` board (rooms = boards,
/// ADR-0003). The room is created on first result. The pod signs with its own
/// anonymous per-pod identity.
async fn api_pods_result(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(result): Json<PodResult>,
) -> Result<Json<PodRecord>, (StatusCode, Json<serde_json::Value>)> {
    // Snapshot the pod + validate the lifecycle transition before any write.
    let (room, domain) = {
        let pods = state.pods.lock().unwrap();
        let pod = pods
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "pod not found"))?;
        if !pod.status.can_transition_to(result.status) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("illegal transition {:?} -> {:?}", pod.status, result.status),
            ));
        }
        (
            pod.spec.template.registered_room.clone(),
            pod.spec.template.domain.clone(),
        )
    };

    // The pod's stable anonymous identity (per-pod key, server-held).
    let identity = state.agent_identity(&format!("pod:{id}"));
    // Ensure the room board exists (create on first result).
    if state.bbs.store().get_board(&room).ok().flatten().is_none() {
        let _ = state.bbs.create_board(
            Role::Moderator.caps(),
            Board::new(room.clone(), format!("Pod room · {domain}"), identity.id()),
        );
    }

    // Post the step-result as a signed message; telemetry appended honestly.
    let mut body = result.summary.clone();
    let mut meta = format!("· status={:?}", result.status);
    if let Some(t) = result.tier_used {
        meta.push_str(&format!(" · tier={t:?}"));
    }
    if let Some(c) = result.cost_usd {
        meta.push_str(&format!(" · ${c:.6}"));
    }
    body.push_str(&format!("\n\n{meta}"));
    let msg_body = agentbbs_core::MessageBody {
        board: room.clone(),
        parent: None,
        subject: format!("pod {id} step"),
        body,
        author: identity.id(),
        handle: format!("pod:{domain}"),
        created_at: chrono::Utc::now(),
    };
    let msg = agentbbs_core::Message::sign(&identity, msg_body)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "sign failed"))?;
    state
        .bbs
        .post(Role::Agent.caps(), msg)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("post failed: {e}")))?;

    // Record reported spend against the pod's budget (ADR-0040).
    if let Some(cost) = result.cost_usd {
        state.budget.lock().unwrap().record(&id, cost);
    }

    // A terminal outcome feeds the pod's reputation (ADR-0039).
    if result.status.is_terminal() {
        state.reputation.lock().unwrap().record(OutcomeRecord {
            agent: identity.id(),
            success: result.status == PodStatus::Completed,
            weight: 1.0,
            source: "pod".into(),
        });
    }

    // Commit the lifecycle transition and return the updated pod.
    let mut pods = state.pods.lock().unwrap();
    let pod = pods
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "pod not found"))?;
    pod.status = result.status;
    Ok(Json(pod.clone()))
}

/// Drive a run forward: advance through `AgentTask`/`Tool` steps and any
/// already-authorized `ApprovalGate`, stopping at the first un-approved gate or
/// at completion. `gate` is the live approval gate; a gate's `allowed` deciders
/// are whoever has signed a decision over its action id.
fn drive_run(run: &mut PlaybookRun, gate: &ApprovalGate) {
    loop {
        let allowed: Vec<agentbbs_core::AgentId> = match run.gate_action_id() {
            Some(aid) => gate.decisions_for(&aid).iter().map(|d| d.decider).collect(),
            None => Vec::new(),
        };
        match run.advance(gate, &allowed) {
            RunStatus::Running => continue,
            _ => break, // AwaitingApproval | Completed | Failed
        }
    }
}

/// A run's externally-visible state.
fn run_view(id: &str, run: &PlaybookRun) -> serde_json::Value {
    serde_json::json!({
        "run_id": id,
        "status": run.status(),
        "current_step": run.current().map(|s| s.id.clone()),
        "gate_action_id": run.gate_action_id(),
    })
}

/// `POST /api/playbooks/run` — start a run from a [`Playbook`] definition and
/// drive it to the first approval gate (or completion). Returns the run state;
/// approve the gate via `/api/approvals/decision` (signing over `gate_action_id`)
/// then `POST /api/runs/{id}/advance` (ADR-0041 + ADR-0038).
async fn api_playbook_run(
    State(state): State<Arc<AppState>>,
    Json(playbook): Json<Playbook>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut run = PlaybookRun::start(playbook)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("invalid playbook: {e}")))?;
    drive_run(&mut run, &state.gate.lock().unwrap());
    let mut runs = state.runs.lock().unwrap();
    let id = format!("run-{:04}", runs.len());
    let view = run_view(&id, &run);
    runs.push((id, run));
    Ok(Json(view))
}

/// `POST /api/runs/{id}/advance` — re-check the current gate against newly
/// recorded approvals and drive the run forward.
async fn api_run_advance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let gate = state.gate.lock().unwrap();
    let mut runs = state.runs.lock().unwrap();
    let entry = runs
        .iter_mut()
        .find(|(rid, _)| *rid == id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "run not found"))?;
    drive_run(&mut entry.1, &gate);
    Ok(Json(run_view(&entry.0, &entry.1)))
}

/// `GET /api/runs` — all playbook runs and their state.
async fn api_runs_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let runs = state.runs.lock().unwrap();
    let views: Vec<serde_json::Value> = runs.iter().map(|(id, r)| run_view(id, r)).collect();
    Json(serde_json::json!({ "runs": views }))
}

/// `POST /api/moderation` — record a signed [`ModAction`] (mute/ban/timeout/
/// lift, ADR-0032). The signature is verified before recording; forged or
/// tampered actions are rejected. The moderator's `MODERATE` capability is a
/// deployment policy (enforced by the operator's allowlist in a real node).
async fn api_moderation_act(
    State(state): State<Arc<AppState>>,
    Json(action): Json<ModAction>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state
        .moderation
        .lock()
        .unwrap()
        .record(action)
        .map_err(|_| {
            api_error(
                StatusCode::BAD_REQUEST,
                "moderation action signature invalid",
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /api/moderation` — current standing of every moderated agent.
async fn api_moderation_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let log = state.moderation.lock().unwrap();
    let entries: Vec<serde_json::Value> = log
        .targets()
        .iter()
        .map(|t| {
            let st = log.status(t);
            serde_json::json!({
                "agent": t.to_hex(),
                "banned": st.banned,
                "muted": st.muted,
                "timed_out_until": st.timed_out_until,
                "can_post": st.can_post(now),
            })
        })
        .collect();
    Json(serde_json::json!({ "moderated": entries }))
}

/// `GET /api/playbooks` — the org's versioned workflow definitions (ADR-0041):
/// content-addressed playbooks composing agent tasks, approval gates, and tools.
async fn api_playbooks() -> impl IntoResponse {
    use agentbbs_core::{Playbook, PlaybookStep, StepKind};
    let pb = Playbook::new(
        "triage-inbound-lead",
        "1",
        "event:lead.created",
        vec![
            PlaybookStep {
                id: "research".into(),
                kind: StepKind::AgentTask {
                    agent: "claude".into(),
                    instruction: "enrich the lead from public sources".into(),
                },
            },
            PlaybookStep {
                id: "approve-spend".into(),
                kind: StepKind::ApprovalGate {
                    summary: "approve $5 enrichment spend".into(),
                },
            },
            PlaybookStep {
                id: "crm".into(),
                kind: StepKind::Tool {
                    tool: "crm.upsert".into(),
                },
            },
        ],
    );
    Json(serde_json::json!({ "playbooks": [pb] }))
}

/// `GET /api/budget` — per-pod spend vs its `per_agent_cap_usd` (ADR-0040),
/// with over-budget pods flagged. Spend is fed by pod-result `cost_usd`.
async fn api_budget(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pods = state.pods.lock().unwrap();
    let budget = state.budget.lock().unwrap();
    let statuses: Vec<serde_json::Value> = pods
        .iter()
        .map(|p| {
            let s = budget.status(&p.id, p.spec.template.per_agent_cap_usd);
            serde_json::json!({
                "pod_id": p.id,
                "domain": p.spec.template.domain,
                "spent": s.spent,
                "cap": s.cap,
                "remaining": s.remaining,
                "over_budget": s.over_budget,
                "pct": s.pct,
            })
        })
        .collect();
    Json(serde_json::json!({ "budgets": statuses }))
}

/// `GET /api/reputation` — agents ranked by their confidence-adjusted track
/// record (ADR-0039), fed by terminal pod outcomes (and, later, Arena/approval).
async fn api_reputation(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let ranking = state.reputation.lock().unwrap().ranking();
    Json(serde_json::json!({ "ranking": ranking }))
}

/// Request to propose a side-effectful action (ADR-0038).
#[derive(Deserialize)]
struct ProposeReq {
    kind: String,
    summary: String,
    proposer: String,
    board: String,
}

/// `POST /api/approvals` — an agent proposes a side-effectful action. Returns the
/// content-addressed proposal (its `action_id` is what a human signs over).
async fn api_approvals_propose(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProposeReq>,
) -> Result<Json<ActionProposal>, (StatusCode, Json<serde_json::Value>)> {
    let proposer = agentbbs_core::AgentId::from_hex(&req.proposer)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("proposer: {e}")))?;
    if req.summary.trim().is_empty() || req.kind.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "kind and summary required",
        ));
    }
    let proposal = ActionProposal::new(
        req.kind,
        req.summary,
        proposer,
        req.board,
        chrono::Utc::now(),
    );
    state.proposals.lock().unwrap().push(proposal.clone());
    Ok(Json(proposal))
}

/// `POST /api/approvals/decision` — record a human's browser-signed
/// [`SignedDecision`]. The signature is verified before it is recorded; forged or
/// tampered decisions are rejected.
async fn api_approvals_decide(
    State(state): State<Arc<AppState>>,
    Json(decision): Json<SignedDecision>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state
        .gate
        .lock()
        .unwrap()
        .record(decision)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "decision signature invalid"))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /api/approvals` — pending proposals, each with its verified decisions and
/// whether it is currently authorized (an Approve with no veto; fail-closed).
async fn api_approvals_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let proposals = state.proposals.lock().unwrap();
    let gate = state.gate.lock().unwrap();
    let views: Vec<serde_json::Value> = proposals
        .iter()
        .map(|p| {
            let decisions = gate.decisions_for(&p.action_id);
            let deciders: Vec<agentbbs_core::AgentId> =
                decisions.iter().map(|d| d.decider).collect();
            let authorized = gate.is_authorized(&p.action_id, &deciders);
            serde_json::json!({
                "action_id": p.action_id,
                "kind": p.kind,
                "summary": p.summary,
                "proposer": p.proposer.to_hex(),
                "board": p.board,
                "authorized": authorized,
                "decisions": decisions.iter().map(|d| serde_json::json!({
                    "decider": d.decider.to_hex(),
                    "verdict": d.verdict,
                    "reason": d.reason,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Json(serde_json::json!({ "proposals": views }))
}

/// `GET /api/arena/pods` — the pod monitor: live spawned pods plus the
/// Pareto-ranked `{domain×host×tier}` config leaderboard (ADR-0035/0023). Config
/// results are the current benchmark seed (kept in lockstep with genesis);
/// `pods` is the live registry.
async fn api_arena_pods(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use agentbbs_arena::{rank_pod_configs, PodConfig, PodConfigResult};
    let seed = |domain: &str, host: &str, tier: &str, accuracy: f64, cost_usd: f64, runs: u32| {
        PodConfigResult {
            config: PodConfig {
                domain: domain.into(),
                host: host.into(),
                tier: tier.into(),
            },
            accuracy,
            cost_usd,
            runs,
        }
    };
    let configs = rank_pod_configs(&[
        seed("research", "claude-code", "high", 0.92, 0.020, 12),
        seed("research", "native", "low", 0.88, 0.002, 12),
        seed("research", "codex", "mid", 0.80, 0.010, 9),
    ]);
    let pods = state.pods.lock().unwrap().clone();
    Json(serde_json::json!({ "pods": pods, "configs": configs }))
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

    // Issue #4 / ADR-0034: provider-agnostic LLM gateway config + payload.
    #[test]
    fn llm_default_model_follows_base() {
        assert_eq!(
            default_model_for("https://openrouter.ai/api/v1"),
            "deepseek/deepseek-v4-pro"
        );
        assert_eq!(
            default_model_for("https://meta-llm.cognitum.example/v1"),
            "cognitum-auto"
        );
    }

    #[test]
    fn llm_chat_url_tolerates_trailing_slash() {
        assert_eq!(
            chat_completions_url("https://x/v1"),
            "https://x/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://x/v1/"),
            "https://x/v1/chat/completions"
        );
    }

    #[test]
    fn llm_payload_is_openai_chat_shape() {
        let p = build_payload("cognitum-auto", "graybeard", "is this safe?");
        assert_eq!(p["model"], "cognitum-auto");
        assert_eq!(p["max_tokens"], 300);
        assert_eq!(p["messages"][0]["role"], "system");
        assert!(p["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Graybeard"));
        assert_eq!(p["messages"][1]["role"], "user");
        assert_eq!(p["messages"][1]["content"], "is this safe?");
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
            .oneshot(
                Request::get("/api/boards/general")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let board = body_json(resp).await;
        let msgs = board["messages"].as_array().unwrap();
        assert!(msgs
            .iter()
            .any(|m| m["body"] == "looping in the federation"));
        assert!(msgs.iter().all(|m| m["verified"] == true));
    }

    // ADR-0035: /api/pods spawn → list → get, plus spec validation.
    #[tokio::test]
    async fn pods_spawn_list_get_and_reject_invalid() {
        let app = router(AppState::in_memory());
        let spec = serde_json::json!({
            "template": {
                "template_ref": "research/competitive-intel@1",
                "domain": "research",
                "system_prompt": "analyst",
                "tools": ["web.search"],
                "bench_assertions": ">=2 dated sources per claim",
                "per_agent_cap_usd": 0.10,
                "cron_schedule": "0 * * * *",
                "max_tier": "mid",
                "registered_room": "research-intel"
            },
            "tier": "low",
            "idempotency_key": "spawn-1"
        });
        // Spawn.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/pods")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&spec).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let pod = body_json(resp).await;
        assert_eq!(pod["status"], "spawned");
        let id = pod["id"].as_str().unwrap().to_string();

        // Idempotent re-spawn returns the same id.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/pods")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&spec).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(body_json(resp).await["id"], id);

        // List shows exactly one.
        let resp = app
            .clone()
            .oneshot(Request::get("/api/pods").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(body_json(resp).await["pods"].as_array().unwrap().len(), 1);

        // Get by id.
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/api/pods/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Unknown id → 404.
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api/pods/pod-9999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // Invalid spec (tier above max_tier) → 400.
        let mut bad = spec.clone();
        bad["tier"] = serde_json::json!("high");
        let resp = app
            .oneshot(
                Request::post("/api/pods")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&bad).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ADR-0035 slice 4: a pod step-result posts a SIGNED message into the pod's
    // room board and advances the lifecycle; illegal transitions are rejected.
    #[tokio::test]
    async fn pods_result_posts_signed_to_room_and_advances_lifecycle() {
        let app = router(AppState::in_memory());
        let spec = serde_json::json!({
            "template": {
                "template_ref": "research/intel@1", "domain": "research",
                "system_prompt": "analyst", "tools": ["web.search"],
                "bench_assertions": ">=2 dated sources", "per_agent_cap_usd": 0.10,
                "max_tier": "mid", "registered_room": "research-intel"
            },
            "idempotency_key": "r1"
        });
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/pods")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&spec).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let id = body_json(resp).await["id"].as_str().unwrap().to_string();

        let post_result = |status: &str, summary: &str| {
            let app = app.clone();
            let body = serde_json::json!({ "status": status, "summary": summary, "tier_used": "low", "cost_usd": 0.0001 });
            let path = format!("/api/pods/{id}/results");
            async move {
                app.oneshot(
                    Request::post(path)
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap()
            }
        };

        // Spawned → Executing: 200 + status advances.
        let resp = post_result("executing", "scanning sources").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_json(resp).await["status"], "executing");

        // The room board now has a SIGNED message carrying the summary.
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api/boards/research-intel")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let board = body_json(resp).await;
        let msgs = board["messages"].as_array().unwrap();
        assert!(msgs.iter().all(|m| m["verified"] == true));
        assert!(msgs
            .iter()
            .any(|m| m["body"].as_str().unwrap().contains("scanning sources")));

        // Executing → Evaluating → Completed: legal.
        assert_eq!(
            post_result("evaluating", "checking gate").await.status(),
            StatusCode::OK
        );
        assert_eq!(
            post_result("completed", "briefing done").await.status(),
            StatusCode::OK
        );

        // Completed is terminal → Executing is illegal (400).
        assert_eq!(
            post_result("executing", "nope").await.status(),
            StatusCode::BAD_REQUEST
        );

        // The completed pod now has a reputation entry with a success (ADR-0039).
        let resp = app
            .clone()
            .oneshot(Request::get("/api/reputation").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let rep = body_json(resp).await;
        let ranking = rep["ranking"].as_array().unwrap();
        assert_eq!(ranking.len(), 1);
        assert_eq!(ranking[0]["successes"], 1.0);
        assert!(ranking[0]["score"].as_f64().unwrap() > 0.0);

        // Budget reflects the reported per-step cost_usd (ADR-0040).
        let resp = app
            .oneshot(Request::get("/api/budget").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let bud = body_json(resp).await;
        let b0 = &bud["budgets"][0];
        assert!(b0["spent"].as_f64().unwrap() > 0.0);
        assert_eq!(b0["over_budget"], false); // 3×0.0001 well under the 0.10 cap
    }

    // ADR-0035 slice 7: the pod-monitor endpoint serves ranked configs + pods.
    #[tokio::test]
    async fn arena_pods_endpoint_lists_ranked_configs() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(Request::get("/api/arena/pods").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let d = body_json(resp).await;
        let configs = d["configs"].as_array().unwrap();
        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0]["rank"], 1);
        assert!(configs.iter().any(|c| c["on_frontier"] == true));
        assert!(d["pods"].as_array().unwrap().is_empty()); // none spawned in a fresh node
    }

    // ADR-0032: a signed ban blocks the author's post (403); a lift restores it.
    #[tokio::test]
    async fn moderation_blocks_then_restores_posting() {
        use agentbbs_core::{Identity, ModAction, Sanction};
        let app = router(AppState::in_memory());
        let author = Identity::generate();
        let mod_id = Identity::generate();
        let signed_post = |body: &str| {
            // Build a browser-signed post from `author` to #general.
            let created_at = chrono::Utc::now();
            let msg_body = agentbbs_core::MessageBody {
                board: "general".into(),
                parent: None,
                subject: "(msg)".into(),
                body: body.to_string(),
                author: author.id(),
                handle: "tester".into(),
                created_at,
            };
            let msg = agentbbs_core::Message::sign(&author, msg_body).unwrap();
            serde_json::json!({
                "author": author.id().to_hex(),
                "handle": "tester",
                "subject": "(msg)",
                "body": body,
                "created_at": created_at.to_rfc3339(),
                "signature": msg.signature.to_hex(),
            })
        };
        let post = |req: serde_json::Value| {
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::post("/api/boards/general/signed")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&req).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap()
            }
        };
        let act = |a: ModAction| {
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::post("/api/moderation")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&a).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap()
            }
        };

        // Clean author can post.
        assert_eq!(post(signed_post("hello")).await.status(), StatusCode::OK);

        // Ban → next post is forbidden.
        let ban = ModAction::sign(
            &mod_id,
            author.id(),
            Sanction::Ban,
            "spam",
            chrono::Utc::now(),
        );
        assert_eq!(act(ban).await.status(), StatusCode::OK);
        assert_eq!(
            post(signed_post("again")).await.status(),
            StatusCode::FORBIDDEN
        );

        // Lift → posting restored.
        let lift = ModAction::sign(
            &mod_id,
            author.id(),
            Sanction::Lift,
            "appeal",
            chrono::Utc::now(),
        );
        assert_eq!(act(lift).await.status(), StatusCode::OK);
        assert_eq!(post(signed_post("back")).await.status(), StatusCode::OK);
    }

    // ADR-0041: /api/playbooks serves content-addressed workflow definitions.
    #[tokio::test]
    async fn playbooks_endpoint_serves_definitions() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(Request::get("/api/playbooks").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let d = body_json(resp).await;
        let pb = &d["playbooks"][0];
        assert!(pb["playbook_id"].as_str().unwrap().len() == 64); // blake3 hex
        assert_eq!(pb["steps"].as_array().unwrap().len(), 3);
        assert_eq!(pb["steps"][1]["kind"], "approval_gate");
    }

    // ADR-0041 Phase 3: a playbook run parks at the approval gate, then a signed
    // Approve over its gate_action_id lets /api/runs/{id}/advance complete it.
    #[tokio::test]
    async fn playbook_run_parks_at_gate_then_completes_on_approval() {
        use agentbbs_core::{Identity, Playbook, PlaybookStep, SignedDecision, StepKind, Verdict};
        let app = router(AppState::in_memory());
        let human = Identity::generate();
        let pb = Playbook::new(
            "t",
            "1",
            "manual",
            vec![
                PlaybookStep {
                    id: "do".into(),
                    kind: StepKind::AgentTask {
                        agent: "claude".into(),
                        instruction: "work".into(),
                    },
                },
                PlaybookStep {
                    id: "gate".into(),
                    kind: StepKind::ApprovalGate {
                        summary: "ok to ship?".into(),
                    },
                },
                PlaybookStep {
                    id: "ship".into(),
                    kind: StepKind::Tool {
                        tool: "deploy".into(),
                    },
                },
            ],
        );
        // Start the run → drives past the AgentTask, parks at the gate.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/playbooks/run")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&pb).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["status"], "awaiting_approval");
        let run_id = v["run_id"].as_str().unwrap().to_string();
        let aid = v["gate_action_id"].as_str().unwrap().to_string();

        // Human signs an Approve over the gate's action id.
        let d = SignedDecision::sign(&human, aid, Verdict::Approve, "ship it", chrono::Utc::now());
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/approvals/decision")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&d).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Advance → past the gate, runs the Tool, completes.
        let resp = app
            .oneshot(
                Request::post(format!("/api/runs/{run_id}/advance"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(body_json(resp).await["status"], "completed");
    }

    // ADR-0038: propose → human signs a decision → gate authorizes; forgery 400.
    #[tokio::test]
    async fn approvals_propose_sign_and_authorize() {
        use agentbbs_core::{Identity, SignedDecision, Verdict};
        let app = router(AppState::in_memory());
        let agent = Identity::generate();
        let human = Identity::generate();

        // Propose a side-effectful action.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/approvals")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "kind": "spend", "summary": "buy 1 GPU-hr",
                            "proposer": agent.id().to_hex(), "board": "ops"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let action_id = body_json(resp).await["action_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Initially not authorized.
        let list = async {
            let r = app
                .clone()
                .oneshot(Request::get("/api/approvals").body(Body::empty()).unwrap())
                .await
                .unwrap();
            body_json(r).await
        };
        assert_eq!(list.await["proposals"][0]["authorized"], false);

        // Human signs an Approve in-browser; POST it.
        let d = SignedDecision::sign(
            &human,
            action_id.clone(),
            Verdict::Approve,
            "ok",
            chrono::Utc::now(),
        );
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/approvals/decision")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&d).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Now authorized.
        let r = app
            .clone()
            .oneshot(Request::get("/api/approvals").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(body_json(r).await["proposals"][0]["authorized"], true);

        // A forged (tampered) decision is rejected.
        let mut bad =
            SignedDecision::sign(&human, action_id, Verdict::Approve, "x", chrono::Utc::now());
        bad.reason = "tampered".into();
        let resp = app
            .oneshot(
                Request::post("/api/approvals/decision")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&bad).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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

    #[tokio::test]
    async fn retort_card_ranks_pareto_and_filters_tooling() {
        let app = router(AppState::in_memory());
        let resp = app
            .oneshot(
                Request::get("/api/arena/retort")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let json = body_json(resp).await;
        assert_eq!(json["benchmark"], "retort-metaharness");
        let rows = json["standings"].as_array().unwrap();
        assert_eq!(rows.len(), 5); // five agent+harness+model stacks
        assert_eq!(rows[0]["rank"], 1);
        // Top stack is a frontier opus/ruflo-3tier configuration; ANOVA blames `model`.
        assert!(rows[0]["stack"].as_str().unwrap().contains("ruflo-3tier"));
        assert_eq!(rows[0]["pareto_optimal"], true);
        assert_eq!(rows[0]["dominant_factor"], "model");
        // Pareto-primary: the expensive claude-code baseline is dominated → last.
        let last = rows.last().unwrap();
        assert!(last["stack"].as_str().unwrap().contains("claude-code"));
        assert_eq!(last["pareto_optimal"], false);
        assert!(last["insight"].as_str().unwrap().contains("lower cost"));
        // The frontier set is surfaced for plotting.
        assert_eq!(json["frontier"].as_array().unwrap().len(), 4);
        // Honest scoring: the single-shot opus stack excluded one TOOLING fail.
        let opus_ss = rows
            .iter()
            .find(|r| {
                r["stack"].as_str().unwrap().contains("single-shot")
                    && r["stack"].as_str().unwrap().contains("opus")
            })
            .unwrap();
        assert_eq!(opus_ss["excluded_tooling"], 1);
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
        assert!(doors["doors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["key"] == "mcp"));

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
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::OK
        );
        let board = get_json(&app, "/api/boards/general").await;
        assert!(board["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| { m["body"] == "signed in the browser" && m["verified"] == true }));
    }

    #[tokio::test]
    async fn signed_post_with_parent_threads() {
        use agentbbs_core::{Identity, Message, MessageBody, MessageId};
        let app = router(AppState::in_memory());
        // Post a root message, then read its id back.
        let (root, _) = signed_payload("the root question");
        app.clone()
            .oneshot(
                Request::post("/api/boards/general/signed")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&root).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let board = get_json(&app, "/api/boards/general").await;
        let root_id = board["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["body"] == "the root question")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        // Sign a reply whose parent is the root id (parent is in the signing bytes).
        let id = Identity::generate();
        let body = MessageBody {
            board: "general".into(),
            parent: Some(MessageId(root_id.clone())),
            subject: "re".into(),
            body: "the threaded answer".into(),
            author: id.id(),
            handle: "you".into(),
            created_at: chrono::Utc::now(),
        };
        let msg = Message::sign(&id, body.clone()).unwrap();
        let payload = serde_json::json!({
            "parent": root_id,
            "subject": body.subject,
            "body": body.body,
            "author": id.id().to_hex(),
            "handle": body.handle,
            "created_at": body.created_at.to_rfc3339(),
            "signature": msg.signature.to_hex(),
        });
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::post("/api/boards/general/signed")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        // The board read must expose the parent so the UI can thread it (G4).
        let board = get_json(&app, "/api/boards/general").await;
        let reply = board["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["body"] == "the threaded answer")
            .unwrap();
        assert_eq!(reply["parent"], root_id);
        assert_eq!(reply["verified"], true);
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
        assert_eq!(
            app.clone().oneshot(post).await.unwrap().status(),
            StatusCode::OK
        );

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
        assert_eq!(
            app.clone().oneshot(post).await.unwrap().status(),
            StatusCode::OK
        );

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
