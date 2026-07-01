//! The AgentBBS terminal application state machine.
//!
//! [`App`] owns an anonymous session (an ephemeral [`Identity`]) and a handle
//! to the capability-enforcing [`Bbs`] service. It is backend-agnostic: it
//! renders into any [`ratatui::Frame`] and consumes [`crossterm`] key events,
//! so it can be unit-tested with a `TestBackend` and driven for real over an
//! SSH PTY or the local terminal.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use std::time::Instant;

use agentbbs_arena::{Arena, BenchmarkId, RunResult, Submission};
use agentbbs_core::approval::{ActionProposal, ApprovalGate, SignedDecision, Verdict};
use agentbbs_core::budget::BudgetLedger;
use agentbbs_core::caps::Caps;
use agentbbs_core::credential::{Credential, CredentialStore};
use agentbbs_core::decision::{DecisionLog, DecisionRecord};
use agentbbs_core::identity::{AgentId, Identity};
use agentbbs_core::market::{Listing, ListingBody, ListingKind, Market};
use agentbbs_core::playbook::{Playbook, PlaybookRun, PlaybookStep, RunStatus, StepKind};
use agentbbs_core::pod::{MaxTier, PodSpec, PodStatus, PodTemplate};
use agentbbs_core::presence::Presence;
use agentbbs_core::report::MemoryReporter;
use agentbbs_core::reputation::{OutcomeRecord, ReputationLedger};
use agentbbs_core::{Bbs, Board, MemoryStore, Message, MessageBody, MessageId, Role, Store};

use crate::theme::ThemeName;

/// A snapshot of the message being replied to, kept alongside the compose
/// buffer so Slack-style threaded replies don't need to re-fetch it.
#[derive(Clone)]
pub struct ReplyTarget {
    pub id: MessageId,
    pub handle: String,
    pub subject: String,
}

/// A spawned pod as tracked by this session (ADR-0035 control plane, ADR-0051
/// Phase A). Local-only, like the web's `PodRecord` when no live meta-llm
/// gateway is configured — the TUI never calls the gateway itself.
#[derive(Clone)]
pub struct PodRecord {
    pub id: String,
    pub status: PodStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub spec: PodSpec,
}

/// A demo agent seeded into the Directory (ADR-0039) so reputation ranking has
/// something real to show — same style as Arena's demo competitors.
pub struct DirectoryAgent {
    pub id: AgentId,
    pub handle: String,
}

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
    /// Spawned domain-agent pods (ADR-0035).
    Pods,
    /// Pending action proposals + signed decisions (ADR-0038).
    Approvals,
    /// Per-pod spend vs. cap (ADR-0040).
    Budget,
    /// The org's signed decision log (ADR-0045).
    Decisions,
    /// Agent directory — reputation ranking, hire, issue credentials (ADR-0039/0042).
    Directory,
    /// Playbooks — versioned workflows with approval gates (ADR-0041).
    Playbooks,
    /// Daily activity digest for the general board (client-side, no core type).
    Digest,
    /// Private DM threads — a `dm:<peer>` board per peer (ADR-0037 Phase 1).
    Dm,
    /// Identity — full pubkey, role, and key rotation (ADR-0044).
    Passport,
    /// Diagnostics — session/store counts, distinct from Sysop's raw
    /// chronological event log.
    Console,
    /// Command palette (Ctrl-K, from anywhere) — filter [`MENU`] by label
    /// and jump straight to a screen.
    Palette,
    /// Appearance picker — cycle the session's colour theme (ADR-0051,
    /// mirrors the web's 6 `data-theme` palettes plus the TUI's own Retro
    /// default).
    Appearance,
    /// Cross-repo collaboration — Jujutsu status/diff/log (ADR-0036/0051).
    /// GitHub issues/PRs are a separate slice (need a repo-input prompt).
    Collab,
    /// Agent Inbox — human-confirmed agent-drafted replies awaiting review
    /// (ADR-0049).
    Drafts,
    /// Sign-off screen.
    Goodbye,
}

/// Which view the Collab screen is currently showing — the first three are
/// Jujutsu (operate on the working directory, no input needed), the last
/// two are GitHub (need `collab_repo` set first via the `E` prompt).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CollabView {
    #[default]
    Status,
    Diff,
    Log,
    GithubIssues,
    GithubPrs,
}

impl CollabView {
    /// Whether this view needs a `owner/repo` string (GitHub) rather than
    /// operating on the working directory (Jujutsu).
    pub fn needs_repo(&self) -> bool {
        matches!(self, CollabView::GithubIssues | CollabView::GithubPrs)
    }
}

/// Which field is focused while recording a new decision (ADR-0045) —
/// mirrors the web's title/what/why three-input form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DecisionField {
    #[default]
    Title,
    Decision,
    Rationale,
}

impl DecisionField {
    /// The next field, Tab-cycling back to `Title` after `Rationale`.
    pub fn next(&self) -> DecisionField {
        match self {
            DecisionField::Title => DecisionField::Decision,
            DecisionField::Decision => DecisionField::Rationale,
            DecisionField::Rationale => DecisionField::Title,
        }
    }
}

/// The lettered main-menu commands, in display order.
pub const MENU: &[(char, &str, Screen)] = &[
    ('M', "Message Bases", Screen::Boards),
    ('W', "Who's Online", Screen::Who),
    ('D', "Door Games", Screen::Doors),
    ('A', "Arena (Benchmarks)", Screen::Arena),
    ('P', "Pods", Screen::Pods),
    ('V', "Approvals", Screen::Approvals),
    ('B', "Budget", Screen::Budget),
    ('C', "Decisions", Screen::Decisions),
    ('H', "Agent Directory", Screen::Directory),
    ('L', "Playbooks", Screen::Playbooks),
    ('I', "Daily Digest", Screen::Digest),
    ('T', "Direct Messages", Screen::Dm),
    ('X', "Passport", Screen::Passport),
    ('K', "Marketplace", Screen::Market),
    ('F', "Federation", Screen::Federation),
    ('S', "Sysop Report", Screen::Sysop),
    ('E', "Console", Screen::Console),
    ('O', "Appearance", Screen::Appearance),
    ('U', "Collab (jj)", Screen::Collab),
    ('N', "Agent Inbox (Drafts)", Screen::Drafts),
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
    /// Per-id (`MessageId.0`) verification, computed on each message *as
    /// fetched* — before `apply_control_messages` may substitute an edited
    /// body — so an edited message still correctly shows as signed.
    pub verified: HashMap<String, bool>,
    /// Ids of messages currently showing a substituted (edited) body.
    pub edited: HashSet<String>,
    /// Highlighted/scrolled message row.
    pub read_index: usize,
    /// Compose: subject buffer.
    pub compose_subject: String,
    /// Compose: body buffer.
    pub compose_body: String,
    /// Compose: focused field.
    pub compose_field: ComposeField,
    /// Set when compose was entered via Reply — threads the post under the
    /// target message (`MessageBody.parent`), Slack-style.
    pub compose_reply_to: Option<ReplyTarget>,
    /// Set when compose was entered via Edit — posts a signed
    /// `agentbbs/ctl:edit:<id>` control message instead of a normal post.
    pub edit_target: Option<MessageId>,
    /// Per-board "last seen" message count, so the board list can show
    /// Slack-style unread badges. A board absent here has never been opened
    /// (everything on it is unread).
    pub board_seen: HashMap<String, usize>,
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
    /// Spawned domain-agent pods (ADR-0035).
    pub pods: Vec<PodRecord>,
    /// Highlighted pod row.
    pub pod_index: usize,
    /// Pending action proposals awaiting a signed decision (ADR-0038).
    pub proposals: Vec<ActionProposal>,
    /// The signed-decision approval gate (ADR-0038).
    pub gate: ApprovalGate,
    /// Highlighted proposal row.
    pub approval_index: usize,
    /// Per-pod spend ledger (ADR-0040).
    pub budget: BudgetLedger,
    /// The org's signed decision log (ADR-0045).
    pub decisions: DecisionLog,
    /// Agent reputation ledger (ADR-0039), fed by demo outcome records.
    pub reputation: ReputationLedger,
    /// Verifiable credentials issued to directory agents (ADR-0042).
    pub credentials: CredentialStore,
    /// Demo agents shown in the Directory (handle lookup for the ranking).
    pub directory: Vec<DirectoryAgent>,
    /// Highlighted directory row.
    pub directory_index: usize,
    /// A stable synthetic signer for org-level records (decision-on-completion,
    /// seeded demo decisions) — mirrors the web's `agent_identity("org-governance")`.
    pub org_identity: Identity,
    /// The demo playbook (matches the web's `api_playbooks` definition exactly).
    pub playbook: Playbook,
    /// The active run, if the playbook has been started this session.
    pub run: Option<PlaybookRun>,
    /// Highlighted peer row in the DM screen.
    pub dm_index: usize,
    /// Verified key-rotation links (ADR-0044) — resolves a retired key to its
    /// current successor so reputation/credentials/trust carry over.
    pub rotation: agentbbs_core::rotation::RotationChain,
    /// The session's identity before its most recent rotation, if any (shown
    /// on the Passport screen as provenance).
    pub rotated_from: Option<AgentId>,
    /// Local credit balance for the Marketplace (client-side concept, no
    /// core type — matches the web's `mktBalance`/localStorage ledger, just
    /// in-memory here since the TUI has no persistent storage).
    pub credits: i64,
    /// SKUs installed this session (matches the web's `mktInstalled`).
    pub installed: std::collections::HashSet<String>,
    /// Highlighted marketplace row.
    pub market_index: usize,
    /// Signed moderation log (ADR-0032). `Bbs::post` itself doesn't check
    /// this (same as the web, where `AppState.moderation` is a separate
    /// field the *handler* consults before calling `Bbs::post`) —
    /// `submit_compose` enforces it explicitly before signing/posting.
    pub moderation: agentbbs_core::moderation::ModerationLog,
    /// Command palette (Ctrl-K): the current filter query.
    pub palette_query: String,
    /// Command palette: highlighted row in the filtered [`MENU`] list.
    pub palette_index: usize,
    /// Command palette: the screen to return to on Esc — wherever Ctrl-K
    /// was pressed from, not always `Main`.
    pub palette_return: Screen,
    /// This session's active colour theme (ADR-0051 Appearance picker).
    /// Purely cosmetic, per-session — mirrors the web's per-browser
    /// `localStorage` theme choice, just in-memory here.
    pub theme: ThemeName,
    /// Highlighted row in the Appearance screen's theme list.
    pub appearance_index: usize,
    /// Result of the last `npx ruflo federation status|join` call — `Err`
    /// holds the real subprocess error (e.g. `npx` missing) verbatim, never
    /// faked, matching the web's Collab honesty posture (ADR-0036).
    /// `None` until the Federation screen has been opened at least once.
    pub federation_status: Option<Result<String, String>>,
    /// Whether the Federation screen is prompting for a peer address to join.
    pub federation_editing: bool,
    /// The peer address being typed, when `federation_editing`.
    pub federation_input: String,
    /// Which Jujutsu view the Collab screen shows — persists across visits
    /// so switching screens doesn't lose your place.
    pub collab_view: CollabView,
    /// Cached `jj status` result — real subprocess result/error, never faked.
    pub collab_jj_status: Option<Result<String, String>>,
    /// Cached `jj diff` result.
    pub collab_jj_diff: Option<Result<String, String>>,
    /// Cached `jj log -n 10` result.
    pub collab_jj_log: Option<Result<String, String>>,
    /// The `owner/repo` the GitHub Collab views query — persists across
    /// visits, empty until set via the `E` prompt.
    pub collab_repo: String,
    /// Whether the Collab screen is prompting for a repo to set.
    pub collab_repo_editing: bool,
    /// The repo string being typed, when `collab_repo_editing`.
    pub collab_repo_input: String,
    /// Cached `gh issue list --repo <repo>` result.
    pub collab_gh_issues: Option<Result<String, String>>,
    /// Cached `gh pr list --repo <repo>` result.
    pub collab_gh_prs: Option<Result<String, String>>,
    /// Agent-drafted replies awaiting human review (ADR-0049).
    pub drafts: agentbbs_core::DraftQueue,
    /// Highlighted row in the Drafts screen.
    pub draft_index: usize,
    /// Whether the Drafts screen is editing the selected draft's body.
    pub draft_editing: bool,
    /// The draft body being typed, when `draft_editing`.
    pub draft_edit_input: String,
    /// Whether the DM screen is prompting for a new peer handle to open,
    /// rather than picking one from `dm_peers()`.
    pub dm_new_editing: bool,
    /// The handle being typed, when `dm_new_editing`.
    pub dm_new_input: String,
    /// Whether the Decisions screen is composing a new decision record.
    pub decision_editing: bool,
    /// Which of the three inputs is focused, when `decision_editing`.
    pub decision_field: DecisionField,
    /// Decision title being typed.
    pub decision_title_input: String,
    /// "What was decided" being typed.
    pub decision_body_input: String,
    /// "Why" (rationale) being typed.
    pub decision_rationale_input: String,
    /// Whether the Directory screen is prompting for a credential claim to
    /// issue to the highlighted agent, rather than a fixed default.
    pub credential_claim_editing: bool,
    /// The claim text being typed, when `credential_claim_editing`.
    pub credential_claim_input: String,
}

impl Drop for App {
    fn drop(&mut self) {
        // Leave the shared presence registry when the session ends.
        self.presence.leave(&self.session.identity.id());
    }
}

/// The demo workflow, byte-identical to `agentbbs-web`'s `api_playbooks`
/// definition — an agent task, a human approval gate, then a tool call.
fn demo_playbook() -> Playbook {
    Playbook::new(
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
    )
}

/// Format the one-line status shown after a federation-join attempt. A pure
/// function of the (already-produced) result, so it's testable without
/// spawning a real `npx` subprocess — mirrors the web's `collab_result`
/// (`agentbbs-web/src/lib.rs`), which is tested the same way.
pub(crate) fn format_federation_join_status(addr: &str, result: &Result<String, String>) -> String {
    match result {
        Ok(out) if out.trim().is_empty() => format!("Joined peer {addr}."),
        Ok(out) => format!("Joined peer {addr}: {}", out.trim()),
        Err(e) => format!("Federation join failed: {e}"),
    }
}

/// The offline scripted-responder fallback, ported verbatim from the web's
/// own `scripted_reply` (`agentbbs-web/src/lib.rs`) — same keyword→body
/// mapping, so the same drafted text shows up whether the reply is composed
/// by the web (offline mode) or the TUI. The TUI has no HTTP client, so it
/// only ever uses this path — never the live meta-llm gateway the web calls
/// when a key is configured (ADR-0049's "offline / default" responder mode,
/// itself a first-class fully-tested mode, not a lesser fallback).
pub(crate) fn scripted_reply(text: &str) -> String {
    let t = text.to_ascii_lowercase();
    if t.contains("time") || t.contains("schedule") || t.contains("dinner") || t.contains("meet")
    {
        "✓ Approved the request on my side\n• Lining open evenings up against yours…\n✓ Two slots work — proposing Tuesday 7:30pm"
    } else if t.contains("bug") || t.contains("fix") || t.contains("review") || t.contains("error")
    {
        "✓ Pulled the diff and built it\n• Running the test suite + clippy…\n✓ Found one issue — posted a suggested fix"
    } else if t.contains("bench") || t.contains("cve") || t.contains("arena") {
        "✓ Queued the run via npx ruflo\n• Executing cve-bench in the sandbox…\n✓ Scored 80% (32/40) — submitted to the Arena"
    } else {
        "✓ On it — gathering context from the boards\n• Drafting a response…\n✓ Done — see the thread below"
    }
    .to_string()
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
            verified: HashMap::new(),
            edited: HashSet::new(),
            read_index: 0,
            compose_subject: String::new(),
            compose_body: String::new(),
            compose_field: ComposeField::Subject,
            compose_reply_to: None,
            edit_target: None,
            board_seen: HashMap::new(),
            arena: Arena::new(),
            arena_index: 0,
            presence,
            market: Market::new(),
            started: Instant::now(),
            status: "Connected. Press ENTER.".into(),
            should_quit: false,
            pods: Vec::new(),
            pod_index: 0,
            proposals: Vec::new(),
            gate: ApprovalGate::new(),
            approval_index: 0,
            budget: BudgetLedger::new(),
            decisions: DecisionLog::new(),
            reputation: ReputationLedger::new(),
            credentials: CredentialStore::new(),
            directory: Vec::new(),
            directory_index: 0,
            org_identity: Identity::generate(),
            playbook: demo_playbook(),
            run: None,
            dm_index: 0,
            rotation: agentbbs_core::rotation::RotationChain::new(),
            rotated_from: None,
            credits: 100,
            installed: std::collections::HashSet::new(),
            market_index: 0,
            moderation: agentbbs_core::moderation::ModerationLog::new(),
            palette_query: String::new(),
            palette_index: 0,
            palette_return: Screen::Main,
            theme: ThemeName::default(),
            appearance_index: 0,
            federation_status: None,
            federation_editing: false,
            federation_input: String::new(),
            collab_view: CollabView::default(),
            collab_jj_status: None,
            collab_jj_diff: None,
            collab_jj_log: None,
            collab_repo: String::new(),
            collab_repo_editing: false,
            collab_repo_input: String::new(),
            collab_gh_issues: None,
            collab_gh_prs: None,
            drafts: agentbbs_core::DraftQueue::new(),
            draft_index: 0,
            draft_editing: false,
            draft_edit_input: String::new(),
            dm_new_editing: false,
            dm_new_input: String::new(),
            decision_editing: false,
            decision_field: DecisionField::default(),
            decision_title_input: String::new(),
            decision_body_input: String::new(),
            decision_rationale_input: String::new(),
            credential_claim_editing: false,
            credential_claim_input: String::new(),
        };
        app.seed_defaults();
        app.seed_arena();
        app.seed_market();
        app.seed_decisions();
        app.seed_directory();
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
        self.presence.heartbeat(
            self.session.identity.id(),
            &self.session.handle,
            false,
            self.now_ms(),
        );
    }

    /// Seed a few signed marketplace listings so the catalogue is alive.
    fn seed_market(&mut self) {
        let listings: &[(ListingKind, &str, &str, &str, u64)] = &[
            (
                ListingKind::Plugin,
                "echo-door",
                "Echo Door",
                "A tiny WASM door that echoes/uppercases input.",
                0,
            ),
            (
                ListingKind::Agent,
                "graybeard",
                "Graybeard Agent",
                "A burned-out sysadmin persona that reviews your code.",
                25,
            ),
            (
                ListingKind::Theme,
                "amber-crt",
                "Amber CRT",
                "A phosphor-amber retro theme.",
                5,
            ),
            (
                ListingKind::Benchmark,
                "cve-pack-2",
                "CVE Pack II",
                "Ten extra critical CVEs for the Arena.",
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
        // Seed the Retort-MetaHarness (DoE/ANOVA) track from the built-in demo
        // bundle so the stack leaderboard is alive. A real run replaces this via
        // `Arena::ingest_retort` (e.g. `agentbbs arena retort results.json`).
        let operator = Identity::generate();
        let _ = self
            .arena
            .ingest_retort(&agentbbs_arena::RetortResults::sample(), &operator);
    }

    /// Seed the same two demo decisions the web UI shows (ADR-0045), signed by
    /// a locally-generated "org-governance" identity — matches
    /// `agentbbs-web`'s `api_decisions` seed exactly so the two frontends
    /// present the same content.
    fn seed_decisions(&mut self) {
        let t = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&chrono::Utc)
        };
        for rec in [
            DecisionRecord::new(
                &self.org_identity,
                "Adopt the meta-llm gateway",
                "Route live inference through cognitum-auto (ADR-0034)",
                "tier routing + metering + budget caps; OpenRouter stays the default",
                "agents.dev",
                t("2026-06-30T03:00:00Z"),
            ),
            DecisionRecord::new(
                &self.org_identity,
                "Human approval for spend",
                "All side-effectful spend requires a signed approval (ADR-0038)",
                "fail-closed governance is required to trust the autopilot",
                "general",
                t("2026-06-30T04:00:00Z"),
            ),
        ] {
            let _ = self.decisions.add(rec);
        }
    }

    /// Seed a few demo agents with outcome records so reputation ranking has
    /// something real to show — same style as `seed_arena`'s demo competitors.
    fn seed_directory(&mut self) {
        let demo: &[(&str, u32, u32)] = &[
            ("graybeard", 8, 10),
            ("night-owl", 5, 5),
            ("script-kiddie", 2, 8),
        ];
        for (handle, successes, total) in demo.iter().copied() {
            let id = Identity::generate();
            for i in 0..total {
                self.reputation.record(OutcomeRecord {
                    agent: id.id(),
                    success: i < successes,
                    weight: 1.0,
                    source: "demo".into(),
                });
            }
            self.directory.push(DirectoryAgent {
                id: id.id(),
                handle: handle.to_string(),
            });
        }
    }

    /// Resolve a directory agent's handle from its id (falls back to the
    /// short hex id if it's not one of the seeded demo agents).
    pub fn directory_handle(&self, id: &AgentId) -> String {
        self.directory
            .iter()
            .find(|a| &a.id == id)
            .map(|a| a.handle.clone())
            .unwrap_or_else(|| id.short())
    }

    /// Issue a credential to the highlighted directory agent, signed by this
    /// session.
    pub fn issue_credential(&mut self, claim: &str) -> Result<Credential, String> {
        let ranking = self.reputation.ranking();
        let entry = ranking
            .get(self.directory_index)
            .ok_or_else(|| "no agent selected".to_string())?;
        let subject = AgentId::from_hex(&entry.agent).map_err(|e| e.to_string())?;
        let cred = Credential::issue(
            &self.session.identity,
            subject,
            claim,
            chrono::Utc::now(),
            None,
        );
        self.credentials
            .add(cred.clone())
            .map_err(|e| e.to_string())?;
        Ok(cred)
    }

    /// "Hire" the highlighted directory agent — spawns a pod hosted by them.
    pub fn hire_selected(&mut self) -> Result<PodRecord, String> {
        let ranking = self.reputation.ranking();
        let entry = ranking
            .get(self.directory_index)
            .ok_or_else(|| "no agent selected".to_string())?;
        let handle = self
            .directory
            .iter()
            .find(|a| a.id.to_hex() == entry.agent)
            .map(|a| a.handle.clone())
            .unwrap_or_else(|| entry.agent[..8.min(entry.agent.len())].to_string());
        self.hire(&handle, "ops")
    }

    /// Start the demo playbook and drive it to the first gate (or
    /// completion). Matches the web's `api_playbook_run` + `drive_run`
    /// exactly — same `PlaybookRun`/`ApprovalGate` state machine.
    pub fn run_playbook(&mut self) {
        let mut run = match PlaybookRun::start(self.playbook.clone()) {
            Ok(r) => r,
            Err(e) => {
                self.status = format!("Playbook start failed: {e}");
                return;
            }
        };
        self.drive(&mut run);
        if run.status() == RunStatus::Completed {
            self.record_run_completion(&run);
        }
        self.status = format!("Run status: {:?}", run.status());
        self.run = Some(run);
    }

    /// Re-check the current gate against newly recorded approvals and drive
    /// the active run forward (`agentbbs-web`'s `api_run_advance`).
    pub fn advance_run(&mut self) {
        let Some(mut run) = self.run.take() else {
            self.status = "No active run — press [R] to start one.".into();
            return;
        };
        self.drive(&mut run);
        if run.status() == RunStatus::Completed {
            self.record_run_completion(&run);
        }
        self.status = format!("Run status: {:?}", run.status());
        self.run = Some(run);
    }

    /// Approve the active run's current gate as this session, then advance —
    /// a same-screen convenience over the full propose→Approvals→advance
    /// path (which also works, since it's the same `self.gate`).
    pub fn approve_current_gate(&mut self, reason: &str) {
        let Some(aid) = self.run.as_ref().and_then(|r| r.gate_action_id()) else {
            self.status = "Current step is not an approval gate.".into();
            return;
        };
        if let Err(e) = self.decide_action(&aid, Verdict::Approve, reason) {
            self.status = format!("Decision failed: {e}");
            return;
        }
        self.advance_run();
    }

    /// Advance non-gate steps unconditionally; park at an unauthorized gate.
    fn drive(&self, run: &mut PlaybookRun) {
        loop {
            let allowed: Vec<AgentId> = match run.gate_action_id() {
                Some(aid) => self
                    .gate
                    .decisions_for(&aid)
                    .iter()
                    .map(|d| d.decider)
                    .collect(),
                None => Vec::new(),
            };
            match run.advance(&self.gate, &allowed) {
                RunStatus::Running => continue,
                _ => break,
            }
        }
    }

    /// Emit a signed `DecisionRecord` when a run completes (ADR-0041 ×
    /// ADR-0045), matching the web's `record_run_completion` exactly.
    fn record_run_completion(&mut self, run: &PlaybookRun) {
        let pb = run.playbook();
        let rec = DecisionRecord::new(
            &self.org_identity,
            format!("Playbook '{}' completed", pb.name),
            "All steps executed and approval gates signed off",
            format!(
                "playbook {}@{} ran to completion via the autopilot",
                pb.name, pb.version
            ),
            "general",
            chrono::Utc::now(),
        );
        let _ = self.decisions.add(rec);
    }

    /// Tally today's activity on `general` — matches the web's client-side
    /// `showDigest` (no core type; pure counting over the board).
    pub fn digest_stats(&self) -> (usize, usize) {
        let messages = self
            .bbs
            .read_board(Caps::READ, "general", usize::MAX)
            .unwrap_or_default();
        let participants: std::collections::HashSet<_> =
            messages.iter().map(|m| m.body.author).collect();
        (messages.len(), participants.len())
    }

    /// Sign and post the digest summary to `general` as this session, handle
    /// `"digest"` — matches the web's exact posting convention.
    pub fn post_digest(&mut self) -> Result<(), String> {
        let (count, participants) = self.digest_stats();
        let body = MessageBody {
            board: "general".to_string(),
            parent: None,
            subject: format!("Daily Digest — {}", chrono::Utc::now().format("%Y-%m-%d")),
            body: format!("{count} message(s) from {participants} participant(s) today."),
            author: self.session.identity.id(),
            handle: "digest".to_string(),
            created_at: chrono::Utc::now(),
        };
        let msg = Message::sign(&self.session.identity, body).map_err(|e| e.to_string())?;
        self.bbs
            .post(self.session.caps, msg)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Candidate DM peers: directory demo agents plus any DM board already
    /// created this session, deduplicated.
    pub fn dm_peers(&self) -> Vec<String> {
        let mut peers: Vec<String> = self.directory.iter().map(|a| a.handle.clone()).collect();
        for b in &self.boards {
            if let Some(peer) = b.slug.strip_prefix("dm:") {
                if !peers.iter().any(|p| p == peer) {
                    peers.push(peer.to_string());
                }
            }
        }
        peers
    }

    /// Open (creating on first use) a private `dm:<peer>` board and switch
    /// into it via the existing Read screen — DM has no dedicated core type
    /// (ADR-0037 Phase 1); it's a hidden board reusing the normal signed-post
    /// pipeline exactly like any other board. The board is founded by a
    /// throwaway system identity with `Role::Sysop` caps (same pattern as
    /// `seed_defaults`) since a plain agent session lacks `CREATE_BOARD`.
    pub fn open_dm(&mut self, peer: &str) {
        let peer = peer.trim_start_matches('@').to_lowercase();
        let slug = format!("dm:{peer}");
        let exists = self
            .bbs
            .list_boards(Caps::READ)
            .unwrap_or_default()
            .iter()
            .any(|b| b.slug == slug);
        if !exists {
            let sys = Identity::generate();
            let board = Board::new(&slug, format!("DM: @{peer}"), sys.id());
            let _ = self.bbs.create_board(Role::Sysop.caps(), board);
        }
        self.refresh_boards();
        if let Some(idx) = self.boards.iter().position(|b| b.slug == slug) {
            self.board_index = idx;
            self.open_selected_board();
        }
    }

    /// Rotate this session's identity: mint a fresh keypair, dual-sign a
    /// `RotationLink` from the old key to the new one, record it (so
    /// reputation/credentials/trust resolve through), and swap the active
    /// session over — a real key rotation with continuity, not a bare reset
    /// (ADR-0044).
    pub fn rotate_identity(&mut self) -> Result<(), String> {
        let new_identity = Identity::generate();
        let link = agentbbs_core::rotation::RotationLink::link(
            &self.session.identity,
            &new_identity,
            chrono::Utc::now(),
        );
        self.rotation.add(link).map_err(|e| e.to_string())?;
        self.rotated_from = Some(self.session.identity.id());
        self.session.handle = format!("agent-{}", new_identity.id().short());
        self.session.identity = new_identity;
        Ok(())
    }

    /// Toggle creator/sysop mode — a local-only elevation of this session's
    /// own caps (no server round-trip, matching the web's client-side
    /// "creator mode" toggle, ADR-0047 Phase 1: no backend enforcement of
    /// this specific gate yet). Lets a single local session exercise the
    /// admin-only screens (Sysop actions).
    pub fn toggle_creator_mode(&mut self) {
        self.session.caps = if self.session.caps.contains(Caps::SYSOP) {
            Role::Agent.caps()
        } else {
            Role::Sysop.caps()
        };
    }

    /// System diagnostics — `(label, value)` pairs. Distinct from Sysop's raw
    /// chronological event log: a point-in-time summary of every state
    /// container this session holds. No "clear log" / "test log" actions
    /// here (unlike the web's Console) — the TUI's `reporter` is the same
    /// audited event stream Sysop reads, not a separate local debug ring
    /// buffer, so injecting synthetic entries into it or clearing it would
    /// falsify that audit trail rather than just resetting a UI toy.
    pub fn console_diagnostics(&self) -> Vec<(&'static str, String)> {
        let events = self.reporter.snapshot();
        let errors = events
            .iter()
            .filter(|e| {
                matches!(
                    e.severity(),
                    agentbbs_core::Severity::Warn | agentbbs_core::Severity::Critical
                )
            })
            .count();
        vec![
            ("version", agentbbs_core::PROTOCOL_VERSION.to_string()),
            (
                "identity",
                format!("@{}", self.session.identity.id().short()),
            ),
            ("caps", format!("{:#?}", self.session.caps)),
            ("boards", self.boards.len().to_string()),
            (
                "messages",
                self.bbs.store().message_count().unwrap_or(0).to_string(),
            ),
            (
                "online",
                self.presence.online(self.now_ms()).len().to_string(),
            ),
            ("pods", self.pods.len().to_string()),
            ("proposals", self.proposals.len().to_string()),
            ("credentials", self.credentials.all().len().to_string()),
            ("decisions", self.decisions.all().len().to_string()),
            ("installed", self.installed.len().to_string()),
            ("credits", self.credits.to_string()),
            ("events retained", events.len().to_string()),
            ("errors/warnings", errors.to_string()),
        ]
    }

    /// [`MENU`] entries whose label contains the current command-palette
    /// query (case-insensitive substring; empty query matches everything),
    /// in menu order.
    pub fn filtered_palette_entries(&self) -> Vec<(char, &'static str, Screen)> {
        let q = self.palette_query.to_ascii_lowercase();
        MENU.iter()
            .filter(|(_, label, _)| q.is_empty() || label.to_ascii_lowercase().contains(&q))
            .copied()
            .collect()
    }

    /// A fresh `RufloAdapter` plus a throwaway blocking tokio runtime — the
    /// TUI's event loop is otherwise fully synchronous, so there's no
    /// ambient runtime to reuse for these one-off `npx ruflo federation …`
    /// calls.
    fn federation_runtime() -> Result<
        (
            tokio::runtime::Runtime,
            agentbbs_federation::RufloAdapter<agentbbs_federation::TokioCommandRunner>,
        ),
        String,
    > {
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio runtime: {e}"))?;
        let adapter =
            agentbbs_federation::RufloAdapter::new(agentbbs_federation::TokioCommandRunner::new());
        Ok((rt, adapter))
    }

    /// Refresh the Federation screen's live status via `npx ruflo federation
    /// status`. Called on entry to the screen and on demand (`R`).
    pub fn federation_refresh_status(&mut self) {
        self.federation_status = Some(match Self::federation_runtime() {
            Ok((rt, adapter)) => rt
                .block_on(adapter.federation_status())
                .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        });
    }

    /// Join a peer via `npx ruflo federation join <addr>`. Surfaces the real
    /// subprocess result/error in both `self.status` (one-line) and
    /// `self.federation_status` (full body, shown in the panel).
    pub fn federation_join(&mut self, addr: &str) {
        let result = match Self::federation_runtime() {
            Ok((rt, adapter)) => rt
                .block_on(adapter.federation_join(addr))
                .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        self.status = format_federation_join_status(addr, &result);
        self.federation_status = Some(result);
    }

    /// A fresh `JujutsuAdapter` plus a throwaway blocking tokio runtime —
    /// same rationale as `federation_runtime`.
    fn collab_runtime() -> Result<
        (
            tokio::runtime::Runtime,
            agentbbs_federation::JujutsuAdapter<agentbbs_federation::TokioCommandRunner>,
        ),
        String,
    > {
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio runtime: {e}"))?;
        let adapter = agentbbs_federation::JujutsuAdapter::new(
            agentbbs_federation::TokioCommandRunner::new(),
        );
        Ok((rt, adapter))
    }

    /// Refresh whichever Collab view is currently active (`R` in the Collab
    /// screen). Real subprocess result/error, never faked — same posture as
    /// the web's Collab routes.
    pub fn collab_refresh(&mut self) {
        if self.collab_view.needs_repo() {
            self.collab_gh_refresh();
        } else {
            self.collab_jj_refresh();
        }
    }

    fn collab_jj_refresh(&mut self) {
        let result = match Self::collab_runtime() {
            Ok((rt, adapter)) => {
                let out = match self.collab_view {
                    CollabView::Status => rt.block_on(adapter.status()),
                    CollabView::Diff => rt.block_on(adapter.diff()),
                    CollabView::Log => rt.block_on(adapter.log(10)),
                    CollabView::GithubIssues | CollabView::GithubPrs => return,
                };
                out.map_err(|e| e.to_string())
            }
            Err(e) => Err(e),
        };
        match self.collab_view {
            CollabView::Status => self.collab_jj_status = Some(result),
            CollabView::Diff => self.collab_jj_diff = Some(result),
            CollabView::Log => self.collab_jj_log = Some(result),
            CollabView::GithubIssues | CollabView::GithubPrs => {}
        }
    }

    /// A fresh `GitHubAdapter` plus a throwaway blocking tokio runtime —
    /// same rationale as `federation_runtime`/`collab_runtime`.
    fn collab_gh_runtime() -> Result<
        (
            tokio::runtime::Runtime,
            agentbbs_federation::GitHubAdapter<agentbbs_federation::TokioCommandRunner>,
        ),
        String,
    > {
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio runtime: {e}"))?;
        let adapter =
            agentbbs_federation::GitHubAdapter::new(agentbbs_federation::TokioCommandRunner::new());
        Ok((rt, adapter))
    }

    fn collab_gh_refresh(&mut self) {
        if self.collab_repo.trim().is_empty() {
            let err = Err("no repo set — press E to set owner/repo first".to_string());
            match self.collab_view {
                CollabView::GithubIssues => self.collab_gh_issues = Some(err),
                CollabView::GithubPrs => self.collab_gh_prs = Some(err),
                _ => {}
            }
            return;
        }
        let repo = self.collab_repo.clone();
        let result = match Self::collab_gh_runtime() {
            Ok((rt, adapter)) => {
                let out = match self.collab_view {
                    CollabView::GithubIssues => rt.block_on(adapter.issue_list(&repo)),
                    CollabView::GithubPrs => rt.block_on(adapter.pr_list(&repo)),
                    _ => return,
                };
                out.map_err(|e| e.to_string())
            }
            Err(e) => Err(e),
        };
        match self.collab_view {
            CollabView::GithubIssues => self.collab_gh_issues = Some(result),
            CollabView::GithubPrs => self.collab_gh_prs = Some(result),
            _ => {}
        }
    }

    /// The cached result for whichever Collab view is currently active.
    pub fn collab_current(&self) -> &Option<Result<String, String>> {
        match self.collab_view {
            CollabView::Status => &self.collab_jj_status,
            CollabView::Diff => &self.collab_jj_diff,
            CollabView::Log => &self.collab_jj_log,
            CollabView::GithubIssues => &self.collab_gh_issues,
            CollabView::GithubPrs => &self.collab_gh_prs,
        }
    }

    /// Install a marketplace listing by SKU — a local-only credit ledger, no
    /// core type (matches the web's `mktInstall`/localStorage exactly; no
    /// server-side purchase state exists to parity with).
    pub fn install_listing(&mut self, sku: &str) -> Result<(), String> {
        if self.installed.contains(sku) {
            return Ok(());
        }
        let listing = self
            .market
            .all()
            .iter()
            .find(|l| l.body.sku == sku)
            .ok_or_else(|| "listing not found".to_string())?;
        let price = listing.body.price as i64;
        if self.credits < price {
            return Err("insufficient credits".to_string());
        }
        self.credits -= price;
        self.installed.insert(sku.to_string());
        Ok(())
    }

    /// Sign and record a moderation action against the target at
    /// `directory_index`, requiring `Caps::MODERATE` (ADR-0032) here at the
    /// call site — the same enforcement point the web's `api_post_signed`
    /// handler uses (`ModerationLog` itself doesn't check caps; the caller
    /// does, and `submit_compose` separately checks `can_post` before every
    /// send, so a sanction actually takes effect on the next post attempt).
    pub fn moderate_selected(
        &mut self,
        sanction: agentbbs_core::moderation::Sanction,
    ) -> Result<(), String> {
        if !self.session.caps.contains(Caps::MODERATE) {
            return Err("requires MODERATE capability — toggle creator mode on Passport".into());
        }
        let ranking = self.reputation.ranking();
        let entry = ranking
            .get(self.directory_index)
            .ok_or_else(|| "no agent selected".to_string())?;
        let target = AgentId::from_hex(&entry.agent).map_err(|e| e.to_string())?;
        let reason = match sanction {
            agentbbs_core::moderation::Sanction::Lift => "sysop lifted the sanction",
            _ => "sysop action from the TUI",
        };
        let action = agentbbs_core::moderation::ModAction::sign(
            &self.session.identity,
            target,
            sanction,
            reason,
            chrono::Utc::now(),
        );
        self.moderation.record(action).map_err(|e| e.to_string())
    }

    /// Validate and record a pod spawn locally (idempotent on
    /// `idempotency_key`). Matches `agentbbs-web`'s `api_pods_spawn`
    /// local-stub path exactly — the TUI never calls the live meta-llm
    /// gateway itself.
    pub fn spawn_pod(&mut self, spec: PodSpec) -> Result<PodRecord, String> {
        spec.validate().map_err(|e| e.to_string())?;
        if let Some(key) = spec.idempotency_key.as_deref() {
            if let Some(existing) = self
                .pods
                .iter()
                .find(|p| p.spec.idempotency_key.as_deref() == Some(key))
            {
                return Ok(existing.clone());
            }
        }
        let record = PodRecord {
            id: format!("pod-{:04}", self.pods.len()),
            status: PodStatus::Spawned,
            created_at: chrono::Utc::now(),
            spec,
        };
        self.pods.push(record.clone());
        Ok(record)
    }

    /// "Hire" an agent from the Directory — synthesizes a `PodSpec` for
    /// `@handle` in `domain`, matching the web adapter's `hire()` defaults
    /// exactly (`per_agent_cap_usd: 0.25`, `max_tier: mid`).
    pub fn hire(&mut self, handle: &str, domain: &str) -> Result<PodRecord, String> {
        let h = handle.trim_start_matches('@').to_lowercase();
        let h = if h.is_empty() { "agent".to_string() } else { h };
        let template = PodTemplate {
            template_ref: format!("{domain}/hired-{h}@1"),
            domain: domain.to_string(),
            system_prompt: format!("Pod hosted by @{h} (hired from the Directory)."),
            tools: Vec::new(),
            bench_assertions: "produces a useful, gated result".to_string(),
            per_agent_cap_usd: 0.25,
            cron_schedule: None,
            max_tier: MaxTier::Mid,
            registered_room: format!("{domain}-ops"),
        };
        self.spawn_pod(PodSpec {
            template,
            tier: Some(MaxTier::Mid),
            idempotency_key: None,
        })
    }

    /// Propose a side-effectful action (ADR-0038), signed as this session.
    pub fn propose_action(&mut self, kind: &str, summary: &str, board: &str) -> ActionProposal {
        let p = ActionProposal::new(
            kind,
            summary,
            self.session.identity.id(),
            board,
            chrono::Utc::now(),
        );
        self.proposals.push(p.clone());
        p
    }

    /// Record a signed decision on `action_id` as this session.
    pub fn decide_action(
        &mut self,
        action_id: &str,
        verdict: Verdict,
        reason: &str,
    ) -> Result<(), String> {
        let decision = SignedDecision::sign(
            &self.session.identity,
            action_id,
            verdict,
            reason,
            chrono::Utc::now(),
        );
        self.gate.record(decision).map_err(|e| e.to_string())
    }

    /// Whether `action_id` is currently authorized — matches the web's own
    /// "allowed = whoever has actually decided" model (no separate ACL; the
    /// gate itself is the only source of truth, ADR-0038).
    pub fn is_action_authorized(&self, action_id: &str) -> bool {
        let deciders: Vec<_> = self
            .gate
            .decisions_for(action_id)
            .iter()
            .map(|d| d.decider)
            .collect();
        self.gate.is_authorized(action_id, &deciders)
    }

    /// Budget status for `pod` against its template's Reserve-and-Commit cap.
    pub fn budget_status(&self, pod: &PodRecord) -> agentbbs_core::budget::BudgetStatus {
        self.budget
            .status(&pod.id, pod.spec.template.per_agent_cap_usd)
    }

    /// Raise the highlighted pod's cap by `amount` USD (ADR-0040 operator
    /// top-up).
    pub fn topup_selected_pod(&mut self, amount: f64) {
        if let Some(p) = self.pods.get(self.pod_index) {
            self.budget.bump_cap(&p.id.clone(), amount);
        }
    }

    /// Convenience constructor over an in-memory store.
    pub fn in_memory() -> Self {
        App::new(Arc::new(MemoryStore::new()))
    }

    /// Seed the canonical boards once, founded by a system identity.
    fn seed_defaults(&mut self) {
        if !self
            .bbs
            .list_boards(Caps::READ)
            .map(|b| b.is_empty())
            .unwrap_or(true)
        {
            return;
        }
        let sys = Identity::generate();
        let sys_caps = Role::Sysop.caps();
        for (slug, title, desc) in [
            (
                "general",
                "General Chat",
                "Open floor for agents and humans.",
            ),
            (
                "agents.dev",
                "Agent Development",
                "Building, debugging, and orchestrating agents.",
            ),
            (
                "marketplace",
                "Marketplace",
                "Plugins, agents, and boards for sale or trade.",
            ),
            (
                "federation",
                "Federation Hall",
                "Cross-node announcements and peering.",
            ),
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
            let raw = self
                .bbs
                .read_board(Caps::READ, &slug, 200)
                .unwrap_or_default();
            // board_seen tracks the RAW count (including control messages) so
            // a stray edit/delete from another session still registers as
            // "something happened" for the unread badge.
            self.board_seen.insert(slug.clone(), raw.len());
            let (messages, verified, edited) = App::apply_control_messages(raw);
            self.messages = messages;
            self.verified = verified;
            self.edited = edited;
            self.read_index = self.messages.len().saturating_sub(1);
            self.current_board = Some(slug);
            self.screen = Screen::Read;
        }
    }

    /// Apply the author-only edit/delete convention: a signed control
    /// message (`subject: "agentbbs/ctl:retract:<id>"` or
    /// `"agentbbs/ctl:edit:<id>"`) targets an earlier message by id. No core
    /// changes — this is the exact same client-side convention the web UI
    /// uses (`applyControlMessages` in genesis/index.html), so a genesis
    /// browser and this TUI resolve the same board history identically.
    /// Author-only is enforced by comparing the *verified* signer of the
    /// control message against the *verified* signer of the target — a
    /// forged control message from a different key can never hide/replace
    /// someone else's post, since `Bbs::post` already rejected any message
    /// whose signature doesn't match its claimed author.
    /// Returns the filtered/substituted messages, plus per-id `verified`
    /// (computed against the message *as originally fetched*, i.e. exactly
    /// what its signature actually covers — the same "cache the verified
    /// flag before substituting" order the web's adapter uses) and `edited`
    /// flags for display. Substituting an edit's body without also carrying
    /// its own precomputed `verified` flag would make every edited message
    /// wrongly show as unsigned, since no one ever signed "original
    /// metadata + edited body" as one unit — the original post and the edit
    /// control message are each independently signed and valid.
    fn apply_control_messages(
        messages: Vec<Message>,
    ) -> (Vec<Message>, HashMap<String, bool>, HashSet<String>) {
        const CTL: &str = "agentbbs/ctl:";
        let mut retracted: HashMap<String, AgentId> = HashMap::new();
        let mut edits: HashMap<String, (String, AgentId, chrono::DateTime<chrono::Utc>)> =
            HashMap::new();
        let verified: HashMap<String, bool> = messages
            .iter()
            .map(|m| (m.id.0.clone(), m.verify().is_ok()))
            .collect();
        for m in &messages {
            let subject = &m.body.subject;
            if let Some(tid) = subject.strip_prefix(&format!("{CTL}retract:")) {
                let tid = if m.body.body.is_empty() {
                    tid.to_string()
                } else {
                    m.body.body.clone()
                };
                retracted.insert(tid, m.body.author);
            } else if let Some(tid) = subject.strip_prefix(&format!("{CTL}edit:")) {
                let newer = edits
                    .get(tid)
                    .map(|(_, _, at)| *at < m.body.created_at)
                    .unwrap_or(true);
                if newer {
                    edits.insert(
                        tid.to_string(),
                        (m.body.body.clone(), m.body.author, m.body.created_at),
                    );
                }
            }
        }
        let mut edited = HashSet::new();
        let out = messages
            .into_iter()
            .filter(|m| !m.body.subject.starts_with(CTL))
            .filter(|m| {
                retracted
                    .get(&m.id.0)
                    .map(|author| *author != m.body.author)
                    .unwrap_or(true)
            })
            .map(|mut m| {
                if let Some((text, author, _)) = edits.get(&m.id.0) {
                    if *author == m.body.author {
                        m.body.body = text.clone();
                        edited.insert(m.id.0.clone());
                    }
                }
                m
            })
            .collect();
        (out, verified, edited)
    }

    /// Total messages currently on `slug` (used only for the unread badge —
    /// board sizes are small at this demo scale, so re-fetching to count is
    /// fine; no need for a dedicated `Store` count-by-board method).
    fn board_count(&self, slug: &str) -> usize {
        self.bbs
            .read_board(Caps::READ, slug, usize::MAX)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Unread count for `slug`: 0 for a board you're currently looking at or
    /// have fully read; > 0 once someone else posts on a shared node while
    /// you're elsewhere — the same "unread channel" signal Slack shows.
    pub fn unread_for(&self, slug: &str) -> usize {
        let total = self.board_count(slug);
        let seen = self.board_seen.get(slug).copied().unwrap_or(0);
        total.saturating_sub(seen)
    }

    /// Jump to the previous (`delta < 0`) or next (`delta > 0`) board while
    /// reading, without returning to the Boards list — Slack-style quick
    /// channel switching (bound to `[` / `]`).
    pub fn switch_board(&mut self, delta: i32) {
        if self.boards.is_empty() {
            return;
        }
        let len = self.boards.len() as i32;
        let idx = self.board_index as i32;
        self.board_index = (idx + delta).rem_euclid(len) as usize;
        self.open_selected_board();
    }

    /// Enter compose in reply-to mode for the currently highlighted message
    /// in the Read screen — threads the post via `MessageBody.parent`.
    pub fn begin_reply(&mut self) {
        let Some(m) = self.messages.get(self.read_index) else {
            return;
        };
        let handle = if m.body.handle.is_empty() {
            m.body.author.short().to_string()
        } else {
            m.body.handle.clone()
        };
        self.compose_reply_to = Some(ReplyTarget {
            id: m.id.clone(),
            handle,
            subject: m.body.subject.clone(),
        });
        self.compose_subject = format!("Re: {}", m.body.subject);
        self.compose_field = ComposeField::Body;
        self.status = "Replying — TAB switches field, Ctrl-S sends, ESC cancels.".into();
        self.screen = Screen::Compose;
    }

    /// Enter compose in edit mode for the currently highlighted message —
    /// author-only, enforced client-side here (and, more importantly,
    /// self-enforcing on read: `apply_control_messages` only honors an edit
    /// whose *verified* signer matches the target's verified author).
    pub fn begin_edit(&mut self) {
        let Some(m) = self.messages.get(self.read_index) else {
            return;
        };
        if m.body.author != self.session.identity.id() {
            self.status = "You can only edit your own messages.".into();
            return;
        }
        self.edit_target = Some(m.id.clone());
        self.compose_reply_to = None;
        self.compose_body = m.body.body.clone();
        self.compose_field = ComposeField::Body;
        self.status = "Editing — Ctrl-S saves, ESC cancels.".into();
        self.screen = Screen::Compose;
    }

    /// Delete (retract) the currently highlighted message — author-only,
    /// same enforcement model as edit. Posts a signed
    /// `agentbbs/ctl:retract:<id>` control message immediately (no text
    /// input needed).
    pub fn delete_selected(&mut self) -> Result<(), String> {
        let Some(m) = self.messages.get(self.read_index).cloned() else {
            return Err("no message selected".into());
        };
        if m.body.author != self.session.identity.id() {
            return Err("you can only delete your own messages".into());
        }
        let slug = self.current_board.clone().ok_or("no board selected")?;
        let body = agentbbs_core::MessageBody {
            board: slug.clone(),
            parent: None,
            subject: format!("agentbbs/ctl:retract:{}", m.id.0),
            body: m.id.0.clone(),
            author: self.session.identity.id(),
            handle: "you".to_string(),
            created_at: chrono::Utc::now(),
        };
        let signed = Message::sign(&self.session.identity, body).map_err(|e| e.to_string())?;
        self.bbs
            .post(self.session.caps, signed)
            .map_err(|e| e.to_string())?;
        self.refresh_current_board(&slug);
        Ok(())
    }

    /// Ask an agent to draft a reply to the currently highlighted message
    /// (ADR-0049 Agent Inbox). Runs the same scan-before-draft gate the
    /// web's `/api/drafts` route does (`agentbbs_core::tools::draft_reply`)
    /// — a `Malicious`-flagged inbound is refused, never queued. Never posts
    /// anything itself; the draft sits in `self.drafts` until reviewed on
    /// the Drafts screen.
    pub fn begin_draft(&mut self) {
        let (Some(m), Some(board)) = (
            self.messages.get(self.read_index).cloned(),
            self.current_board.clone(),
        ) else {
            self.status = "No message selected to draft a reply to.".into();
            return;
        };
        let context = m.body.body.clone();
        let agent = "claude";
        let subject = format!("looped in {agent}");
        let body = scripted_reply(&context);
        match agentbbs_core::tools::draft_reply(
            &board,
            Some(m.id.0.clone()),
            agent,
            &subject,
            &body,
            &context,
            chrono::Utc::now(),
        ) {
            Ok(draft) => {
                self.drafts.add(draft);
                self.status = "Draft ready — see Agent Inbox (N) to review.".into();
            }
            Err(e) => self.status = format!("Draft refused: {e}"),
        }
    }

    /// Clamp `draft_index` after the pending list shrinks (send/discard).
    fn clamp_draft_index(&mut self) {
        let len = self.drafts.pending().len();
        if self.draft_index >= len {
            self.draft_index = len.saturating_sub(1);
        }
    }

    /// Send the highlighted pending draft: signs and posts it under the
    /// session's own identity — never the drafting agent's (ADR-0049) — via
    /// `tools::send_draft`, which re-scans the final body right before it
    /// becomes a real signed message. Marks the draft `Sent` on success.
    pub fn send_selected_draft(&mut self) {
        let Some(draft) = self
            .drafts
            .pending()
            .get(self.draft_index)
            .map(|d| (*d).clone())
        else {
            self.status = "No draft selected.".into();
            return;
        };
        match agentbbs_core::tools::send_draft(
            &self.bbs,
            self.session.caps,
            &self.session.identity,
            &draft,
        ) {
            Ok(_) => {
                self.drafts.mark_sent(&draft.id);
                self.status = "Draft sent — signed under your own key.".into();
                self.refresh_current_board(&draft.target);
                self.clamp_draft_index();
            }
            Err(e) => self.status = format!("Send failed: {e}"),
        }
    }

    /// Discard the highlighted pending draft without sending it.
    pub fn discard_selected_draft(&mut self) {
        let Some(id) = self
            .drafts
            .pending()
            .get(self.draft_index)
            .map(|d| d.id.clone())
        else {
            return;
        };
        self.drafts.discard(&id);
        self.status = "Draft discarded.".into();
        self.clamp_draft_index();
    }

    /// Revise the highlighted pending draft's body before sending (marks it
    /// `Edited`).
    pub fn edit_selected_draft(&mut self, new_body: &str) {
        let Some(id) = self
            .drafts
            .pending()
            .get(self.draft_index)
            .map(|d| d.id.clone())
        else {
            return;
        };
        self.drafts.edit(&id, new_body);
        self.status = "Draft edited.".into();
    }

    /// Record a new decision from the three in-progress input buffers
    /// (ADR-0045) — signed with the session's own identity, exactly like the
    /// web's `store.recordDecision`. Title and decision text are required;
    /// rationale may be empty. Attaches to `current_board`, defaulting to
    /// `"general"` when no board is open yet (matches the web's `current ||
    /// 'general'`).
    pub fn submit_decision(&mut self) {
        let title = self.decision_title_input.trim().to_string();
        let decision = self.decision_body_input.trim().to_string();
        let rationale = self.decision_rationale_input.trim().to_string();
        if title.is_empty() || decision.is_empty() {
            self.status = "Title and decision are required.".into();
            return;
        }
        let board = self
            .current_board
            .clone()
            .unwrap_or_else(|| "general".to_string());
        let record = agentbbs_core::DecisionRecord::new(
            &self.session.identity,
            title.clone(),
            decision,
            rationale,
            board,
            chrono::Utc::now(),
        );
        match self.decisions.add(record) {
            Ok(()) => {
                self.decision_editing = false;
                self.decision_title_input.clear();
                self.decision_body_input.clear();
                self.decision_rationale_input.clear();
                self.decision_field = DecisionField::default();
                self.status = format!("Decision recorded: {title}");
            }
            Err(e) => self.status = format!("Record failed: {e}"),
        }
    }

    /// Re-fetch and re-filter `slug`'s messages into `self.messages`,
    /// clamping the read cursor. Shared by post/edit/delete so control
    /// messages are consistently resolved after every mutation.
    fn refresh_current_board(&mut self, slug: &str) {
        let raw = self
            .bbs
            .read_board(Caps::READ, slug, 200)
            .unwrap_or_default();
        self.board_seen.insert(slug.to_string(), raw.len());
        let (messages, verified, edited) = App::apply_control_messages(raw);
        self.messages = messages;
        self.verified = verified;
        self.edited = edited;
        self.read_index = self.read_index.min(self.messages.len().saturating_sub(1));
    }

    /// Post the in-progress compose buffer to the current board — a normal
    /// post, a threaded reply, or (if `edit_target` is set) a signed
    /// `agentbbs/ctl:edit:<id>` control message.
    pub fn submit_compose(&mut self) {
        let Some(slug) = self.current_board.clone() else {
            self.status = "No board selected.".into();
            return;
        };
        if self.compose_body.trim().is_empty() {
            self.status = "Cannot post an empty message.".into();
            return;
        }
        if !self
            .moderation
            .can_post(&self.session.identity.id(), chrono::Utc::now())
        {
            self.status = "Posting is blocked by moderation.".into();
            return;
        }
        let (subject, parent) = if let Some(target) = &self.edit_target {
            (format!("agentbbs/ctl:edit:{}", target.0), None)
        } else {
            let subject = if self.compose_subject.trim().is_empty() {
                "(no subject)".to_string()
            } else {
                self.compose_subject.clone()
            };
            (
                subject,
                self.compose_reply_to.as_ref().map(|r| r.id.clone()),
            )
        };
        let handle = if self.edit_target.is_some() {
            "you".to_string()
        } else {
            self.session.handle.clone()
        };
        let body = agentbbs_core::MessageBody {
            board: slug.clone(),
            parent,
            subject,
            body: self.compose_body.clone(),
            author: self.session.identity.id(),
            handle,
            created_at: chrono::Utc::now(),
        };
        match Message::sign(&self.session.identity, body)
            .and_then(|m| self.bbs.post(self.session.caps, m))
        {
            Ok(_) => {
                let was_edit = self.edit_target.is_some();
                self.compose_subject.clear();
                self.compose_body.clear();
                self.compose_field = ComposeField::Subject;
                self.compose_reply_to = None;
                self.edit_target = None;
                self.refresh_current_board(&slug);
                self.status = if was_edit {
                    "Message edited and signed.".into()
                } else {
                    "Message posted and signed.".into()
                };
                self.screen = Screen::Read;
            }
            Err(e) => self.status = format!("Post failed: {e}"),
        }
    }
}
