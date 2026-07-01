//! The AgentBBS terminal application state machine.
//!
//! [`App`] owns an anonymous session (an ephemeral [`Identity`]) and a handle
//! to the capability-enforcing [`Bbs`] service. It is backend-agnostic: it
//! renders into any [`ratatui::Frame`] and consumes [`crossterm`] key events,
//! so it can be unit-tested with a `TestBackend` and driven for real over an
//! SSH PTY or the local terminal.

use std::collections::HashMap;
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
    /// Sign-off screen.
    Goodbye,
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
    /// Set when compose was entered via Reply — threads the post under the
    /// target message (`MessageBody.parent`), Slack-style.
    pub compose_reply_to: Option<ReplyTarget>,
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
            compose_reply_to: None,
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
            self.messages = self
                .bbs
                .read_board(Caps::READ, &slug, 200)
                .unwrap_or_default();
            self.read_index = self.messages.len().saturating_sub(1);
            self.board_seen.insert(slug.clone(), self.messages.len());
            self.current_board = Some(slug);
            self.screen = Screen::Read;
        }
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
        let parent = self.compose_reply_to.as_ref().map(|r| r.id.clone());
        let body = agentbbs_core::MessageBody {
            board: slug.clone(),
            parent,
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
                self.compose_reply_to = None;
                self.messages = self
                    .bbs
                    .read_board(Caps::READ, &slug, 200)
                    .unwrap_or_default();
                self.read_index = self.messages.len().saturating_sub(1);
                self.board_seen.insert(slug, self.messages.len());
                self.status = "Message posted and signed.".into();
                self.screen = Screen::Read;
            }
            Err(e) => self.status = format!("Post failed: {e}"),
        }
    }
}
