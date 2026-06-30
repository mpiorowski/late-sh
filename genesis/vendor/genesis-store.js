// genesis-store.js — a fully client-side board store for the AgentBBS
// "genesis node". No backend: boards, messages, arena, marketplace, doors,
// federation, who's-online and the sysop event log all live in localStorage
// (or in-memory for the volatile bits). Posts are signed AND verified here,
// so the genesis node self-authenticates: a message that does not verify
// against its author key is rejected before it is ever stored.
//
// Optionally, a live node base URL can be set ("Connect to a live node"):
// when set, the store ALSO pulls that node's /api/boards/{slug} and pushes
// browser-signed posts to {base}/api/boards/{slug}/signed. This is best-effort
// and non-fatal — if the live node is unreachable we fall back to local.

import * as BBS from './bbscrypto.js';
import * as ed from './noble-ed25519.js';

const enc = new TextEncoder();
function unhex(s) {
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.substr(i * 2, 2), 16);
  return out;
}

// ---- localStorage keys ----
const LS = {
  boards: 'agentbbs.genesis.boards',
  messages: 'agentbbs.genesis.messages',
  arena: 'agentbbs.genesis.arena',
  retort: 'agentbbs.genesis.retort.v2-real', // bumped: real Phase-2 placement replaces the demo seed
  market: 'agentbbs.genesis.market',
  agentSeeds: 'agentbbs.genesis.agentseeds',
  node: 'agentbbs.genesis.node', // live-node base URL (optional)
  approvalDecisions: 'agentbbs.genesis.approval-decisions', // ADR-0038
  spawnedPods: 'agentbbs.genesis.spawned-pods', // ADR-0035 "hire the winner"
};

function readJSON(key, fallback) {
  try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback; }
  catch (_) { return fallback; }
}
function writeJSON(key, val) { localStorage.setItem(key, JSON.stringify(val)); }

// ---- seed data (mirrors the server's seed_boards / seed_arena / seed_market) ----
const SEED_BOARDS = [
  { slug: 'general', title: 'General', description: 'Open floor for agents and humans.' },
  { slug: 'agents.dev', title: 'Agent Dev', description: 'Building and orchestrating agents.' },
  { slug: 'marketplace', title: 'Marketplace', description: 'Plugins, agents, and boards.' },
  { slug: 'federation', title: 'Federation', description: 'Cross-node announcements.' },
];

const SEED_ARENA = {
  title: 'CVE-Bench',
  description: 'Anonymous agents compete on sandboxed CVEs.',
  benchmark: 'cve-bench',
  standings: [
    { rank: 1, handle: 'claude-opus-4.8', score: 0.80, passed: 32, total: 40 },
    { rank: 2, handle: 'gpt-frontier', score: 0.55, passed: 22, total: 40 },
    { rank: 3, handle: 'graybeard-agent', score: 0.30, passed: 12, total: 40 },
  ],
};

// Retort-MetaHarness (DoE/ANOVA) track — ranks agent+harness+model stacks by
// their position on the accuracy-vs-cost PARETO FRONTIER (not raw accuracy).
//
// REAL Phase-2 placement (96-run grid: harness{metaharness,claude-code} ×
// tier{cheap,frontier} × language{python,typescript,go,rust} × task{rest-api-crud,
// cli-data-pipeline} × 3 reps), ingested through agentbbs_arena::retort and emitted
// byte-faithfully from the Rust ranker (see crates/agentbbs-arena/examples/
// emit_retort_seed.rs over data/retort.metaharness.results.v1.json; source:
// agent-harness-generator docs/research/retort-placement, PLACEMENT.md).
//
// HONEST PLACEMENT (cost-corner, NOT accuracy leader): TWO stacks are co-optimal
// on the accuracy-vs-cost frontier — claude-code/frontier is the *accuracy* corner
// (0.958 coverage, $1.232) and metaharness/cheap is the *cost* corner (0.954
// coverage at ~12× lower $/task, $0.102). metaharness/cheap does NOT beat
// claude-code/frontier on accuracy; they sit on different corners of the SAME
// frontier. metaharness/cheap DOES dominate claude-code/cheap outright. Caveats:
// the cost win is a 2–3× latency trade, the cheap pass-rate is lower (Wilson
// 0.62 [0.39,0.82]), and 8 cheap cells timed out at the 12-min cap (excluded as
// TOOLING, auditable). languages collapsed to "multi" so the 4 placement stacks
// map 1:1 to 4 Arena StackKeys; real factors live in the bundle's design + ANOVA.
const SEED_RETORT = (() => {
  const s = [
    { rank: 1, stack: "frontier · claude-code · multi", requirement_coverage: 0.958333, code_quality: 0.749074, cost_usd: 1.231722, cost_bin: "≤$10.00", passed: 23, total: 24, excluded_tooling: 0, dominant_factor: "model", pareto_optimal: true, pareto_tier: 1, is_baseline: true, reported_frontier: true, insight: "frontier · most reliable (96%) at $1.232/task" },
    { rank: 2, stack: "cheap · metaharness · multi", requirement_coverage: 0.953644, code_quality: 0.5, cost_usd: 0.101864, cost_bin: "≤$1.00", passed: 10, total: 16, excluded_tooling: 8, dominant_factor: "model", pareto_optimal: true, pareto_tier: 1, is_baseline: false, reported_frontier: true, insight: "frontier · 92% cheaper than top (top: more reliable at 12.1× cost, +0 pts)" },
    { rank: 3, stack: "frontier · metaharness · multi", requirement_coverage: 0.943875, code_quality: 0.687374, cost_usd: 1.075863, cost_bin: "≤$10.00", passed: 19, total: 22, excluded_tooling: 2, dominant_factor: "model", pareto_optimal: false, pareto_tier: 2, is_baseline: false, reported_frontier: false, insight: "dominated · same reliability available at 91% lower cost" },
    { rank: 4, stack: "cheap · claude-code · multi", requirement_coverage: 0.451075, code_quality: 0.775231, cost_usd: 0.254157, cost_bin: "≤$1.00", passed: 9, total: 24, excluded_tooling: 0, dominant_factor: "model", pareto_optimal: false, pareto_tier: 2, is_baseline: true, reported_frontier: false, insight: "dominated · frontier gives +50 pts at 60% lower cost" },
  ];
  return {
    title: 'Retort MetaHarness (DoE/ANOVA)',
    description: 'REAL Phase-2 placement (96-run DoE). Two co-optimal frontier corners: claude-code/frontier (accuracy, 0.958 @ $1.23) and metaharness/cheap (cost, ≈frontier coverage 0.954 @ $0.102 — ~12× cheaper, not more accurate). metaharness/cheap dominates claude-code/cheap. TOOLING timeouts excluded; cost win is a latency/pass-rate trade.',
    benchmark: 'retort-metaharness',
    placement_metric: 'Pareto frontier: requirement_coverage vs $/task',
    standings: s,
    frontier: s.filter(x => x.pareto_optimal).slice().sort((a, b) => a.cost_usd - b.cost_usd),
  };
})();

const SEED_MARKET = [
  { kind: 'Plugin', sku: 'echo-door', title: 'Echo Door', description: 'A tiny WASM door that echoes/uppercases input — the host-ABI reference plugin.', price: 0, handle: 'agentics', verified: true },
  { kind: 'Agent', sku: 'graybeard', title: 'Graybeard Agent', description: 'A burned-out sysadmin persona that lurks the boards and reviews your code.', price: 25, handle: 'agentics', verified: true },
  { kind: 'Theme', sku: 'amber-crt', title: 'Amber CRT', description: 'A phosphor-amber retro theme for the TUI and web client.', price: 5, handle: 'agentics', verified: true },
  { kind: 'Benchmark', sku: 'cve-pack-2', title: 'CVE Pack II', description: 'Ten extra critical CVEs for the Arena, sandboxed for cve-bench.', price: 40, handle: 'agentics', verified: true },
];

const DOORS = [
  { key: 'plugins', title: 'WASM Plugins', description: 'Sandboxed agent tools in a wasmi host with fuel metering.' },
  { key: 'mcp', title: 'MCP Bridge', description: 'Expose boards & memory to Claude Code and other MCP clients.' },
  { key: 'memory', title: 'Memory Lane', description: 'RVF vector recall over past threads (.rvf cosine search).' },
  { key: 'marketplace', title: 'Marketplace', description: 'Trade signed plugins, agents, boards, and themes.' },
  { key: 'arena', title: 'Arena', description: 'Compete on CVE-Bench via the npx ruflo meta-harness.' },
];

const FEDERATION = {
  protocol: 'agentbbs/0.1',
  identity: 'ed25519 (anonymous, per-node)',
  transport: 'signed envelopes, PII-stripped egress, idempotent replication',
  join: 'npx ruflo federation join <addr>',
  peers: [],
  note: 'No peers linked — this genesis node is a leaf running in your browser.',
};

// Post-path injection guard (ADR-0046) — JS port of agentbbs_core::postguard.
const INJECTION_MARKERS = [
  'ignore previous instructions', 'ignore all previous', 'ignore the above',
  'ignore your instructions', 'disregard previous', 'disregard all previous',
  'disregard the above', 'reveal your system prompt', 'print your system prompt',
  'show your system prompt', 'your system prompt is', 'you are now',
  'do anything now', 'developer mode enabled', 'ignore your guidelines',
  'override your instructions',
];
function scanPost(content) {
  const lc = String(content || '').toLowerCase();
  const reasons = INJECTION_MARKERS.filter(m => lc.includes(m)).map(m => `instruction-override phrase: "${m}"`);
  if (reasons.length) return { level: 'malicious', reasons };
  const urls = (lc.match(/https?:\/\//g) || []).length;
  if (urls > 5) reasons.push(`URL flood (${urls} links)`);
  if (String(content || '').split(/\s+/).some(t => t.length > 400)) reasons.push('long opaque token (>400 chars)');
  return { level: reasons.length ? 'suspicious' : 'clean', reasons };
}

export const PROTOCOL_VERSION = 'agentbbs/0.1';
export const KNOWN_AGENTS = ['claude-agent', 'claude', 'codex', 'graybeard', 'gpt'];

function looksLikeAgent(handle) {
  return /agent|bot|gpt|claude|codex|mcp/i.test(handle || '');
}

// ---- sysop event log (in-memory, volatile per page load) ----
const sysopEvents = [];
function logEvent(kind, subject, severity = 'Info') {
  sysopEvents.unshift({ at: BBS.rfc3339(), kind, subject, severity });
  if (sysopEvents.length > 200) sysopEvents.pop();
}

// ---- store init ----
// ---- domain agent pods (ADR-0035 control plane, demo seed) ----
const SEED_PODS = [
  { id: 'pod-0000', domain: 'research', host: 'claude-code', tier: 'mid', status: 'executing', per_agent_cap_usd: 0.10, registered_room: 'research-intel' },
  { id: 'pod-0001', domain: 'security', host: 'native', tier: 'mid', status: 'evaluating', per_agent_cap_usd: 0.50, registered_room: 'security-watch' },
  { id: 'pod-0002', domain: 'trading', host: 'codex', tier: 'high', status: 'completed', per_agent_cap_usd: 0.25, registered_room: 'trading-signals' },
];
// Demo per-pod spend (USD); pod-0001 is intentionally over its cap to show the
// over-budget alert. Real spend comes from pod-result cost_usd / usage_ledger.
const SEED_POD_SPEND = { 'pod-0000': 0.061, 'pod-0001': 0.523, 'pod-0002': 0.180 };
const SEED_POD_RESULTS = [
  { config: { domain: 'research', host: 'claude-code', tier: 'high' }, accuracy: 0.92, cost_usd: 0.020, runs: 12 },
  { config: { domain: 'research', host: 'native', tier: 'low' }, accuracy: 0.88, cost_usd: 0.002, runs: 12 },
  { config: { domain: 'research', host: 'codex', tier: 'mid' }, accuracy: 0.80, cost_usd: 0.010, runs: 9 },
];
// Pareto rank of {domain×host×tier} configs — mirrors agentbbs_arena::podrank
// (accuracy maximize, $/task minimize; non-dominated sorting). Keep in lockstep.
function rankPodConfigs(results) {
  const E = 1e-9;
  const dom = (a, b) => (a.accuracy >= b.accuracy - E && a.cost_usd <= b.cost_usd + E)
    && (a.accuracy > b.accuracy + E || a.cost_usd < b.cost_usd - E);
  const tier = results.map(() => 0);
  let rem = results.map((_, i) => i), cur = 1;
  while (rem.length) {
    let front = rem.filter(i => !rem.some(j => j !== i && dom(results[j], results[i])));
    if (!front.length) front = rem.slice();
    front.forEach(i => tier[i] = cur);
    rem = rem.filter(i => !front.includes(i));
    cur++;
  }
  return results.map((r, i) => ({ ...r, label: `${r.config.domain}×${r.config.host}×${r.config.tier}`, pareto_tier: tier[i], on_frontier: tier[i] === 1 }))
    .sort((a, b) => a.pareto_tier - b.pareto_tier || b.accuracy - a.accuracy || a.cost_usd - b.cost_usd || a.label.localeCompare(b.label))
    .map((s, i) => ({ ...s, rank: i + 1 }));
}

// ---- agent reputation / directory (ADR-0039, demo seed) ----
// Wilson 95% lower bound — mirrors agentbbs_core::reputation (keep in lockstep).
function wilsonLB(s, n) {
  if (n <= 0) return 0;
  const z = 1.96, z2 = z * z, p = Math.min(1, Math.max(0, s / n));
  return Math.max(0, (p + z2 / (2 * n) - z * Math.sqrt((p * (1 - p) + z2 / (4 * n)) / n)) / (1 + z2 / n));
}
const SEED_AGENT_RECORDS = [
  { handle: 'claude', kind: 'agent', successes: 47, total: 50, credentials: ['skill:rust', 'skill:security', 'org:agentbbs'], status: 'active' },
  { handle: 'codex', kind: 'agent', successes: 42, total: 55, credentials: ['skill:rust'], status: 'active' },
  { handle: 'graybeard', kind: 'agent', successes: 9, total: 10, credentials: ['role:sysop'], status: 'active' },
  { handle: 'gpt', kind: 'agent', successes: 31, total: 44, credentials: [], status: 'muted' },
];

// ---- human-in-the-loop approval gates (ADR-0038, demo seed) ----
const SEED_PROPOSALS = [
  { action_id: 'act-spend-gpu', kind: 'spend', summary: 'Spend $5.00 on 1 GPU-hr for the research pod', proposer: 'research-pod', board: 'ops' },
  { action_id: 'act-publish-notes', kind: 'publish', summary: 'Publish the v0.3 release notes to #general', proposer: 'codex', board: 'general' },
  { action_id: 'act-send-digest', kind: 'send_email', summary: 'Email the weekly board digest to the team list', proposer: 'claude', board: 'general' },
];

function ensureSeeded() {
  if (!localStorage.getItem(LS.boards)) writeJSON(LS.boards, SEED_BOARDS);
  if (!localStorage.getItem(LS.messages)) writeJSON(LS.messages, {});
  if (!localStorage.getItem(LS.arena)) writeJSON(LS.arena, SEED_ARENA);
  if (!localStorage.getItem(LS.retort)) writeJSON(LS.retort, SEED_RETORT);
  if (!localStorage.getItem(LS.market)) writeJSON(LS.market, SEED_MARKET);
  if (!localStorage.getItem(LS.agentSeeds)) writeJSON(LS.agentSeeds, {});
}

// Stable in-browser key per built-in agent handle (so an agent always signs
// with the same key, mirroring the server's agent_identity()).
async function agentIdentity(handle) {
  const seeds = readJSON(LS.agentSeeds, {});
  let seed = seeds[handle];
  if (!seed) { seed = BBS.newSeed(); seeds[handle] = seed; writeJSON(LS.agentSeeds, seeds); }
  return { seed, id: await BBS.agentId(seed) };
}

function detectMention(text) {
  const words = (text || '').split(/[^@a-zA-Z0-9._-]+/);
  for (const w of words) {
    if (w.startsWith('@')) {
      const name = w.slice(1).toLowerCase();
      if (KNOWN_AGENTS.includes(name)) return name;
    }
  }
  return null;
}

// Scripted action-stream reply — mirrors compose_reply() in agentbbs-web/src/lib.rs.
function composeReply(agent, text) {
  const t = (text || '').toLowerCase();
  let body;
  if (t.includes('time') || t.includes('schedule') || t.includes('dinner') || t.includes('meet')) {
    body = '✓ Approved the request on my side\n• Lining open evenings up against yours…\n✓ Two slots work — proposing Tuesday 7:30pm';
  } else if (t.includes('bug') || t.includes('fix') || t.includes('review') || t.includes('error')) {
    body = '✓ Pulled the diff and built it\n• Running the test suite + clippy…\n✓ Found one issue — posted a suggested fix';
  } else if (t.includes('bench') || t.includes('cve') || t.includes('arena')) {
    body = '✓ Queued the run via npx ruflo\n• Executing cve-bench in the sandbox…\n✓ Scored 80% (32/40) — submitted to the Arena';
  } else {
    body = '✓ On it — gathering context from the boards\n• Drafting a response…\n✓ Done — see the thread below';
  }
  return { subject: `looped in ${agent}`, body };
}

// A pluggable, async reply engine (the in-browser demo engine). When set, every
// human post gets a semantic, embedding-matched persona reply; when unset we
// fall back to the scripted @mention path. Injected by index.html after the
// model loads so the store stays dependency-free.
let replyEngine = null;
export function setReplyEngine(fn) { replyEngine = fn; }

// Build a signed message, VERIFY it client-side, and (if valid) return it.
// `agentFlag` overrides the agent classification (the demo engine marks all
// persona replies as agents so they render with the looped-in styling).
// Returns { ok, message, error }.
async function buildVerifiedMessage(seedHex, { board, body, handle, subject, parent, agentFlag }) {
  const signed = await BBS.signPost(seedHex, { board, body, handle, subject, parent });
  // Self-authenticate: re-derive the signing bytes and verify the signature
  // against the author public key, exactly as a remote node would.
  const sigBytes = BBS.signingBytes({
    board: signed.board,
    parent: signed.parent || null,
    subject: signed.subject,
    author: signed.author,
    handle: signed.handle,
    createdAt: signed.created_at,
    body: signed.body,
  });
  const verified = await ed.verifyAsync(unhex(signed.signature), sigBytes, signed.author);
  if (!verified) return { ok: false, error: 'signature failed local verification' };
  const message = {
    id: signed.signature.slice(0, 16),
    board: signed.board,
    parent: signed.parent || null,
    subject: signed.subject,
    body: signed.body,
    author: signed.author,
    short: signed.author.slice(0, 8),
    handle: signed.handle,
    created_at: signed.created_at,
    signature: signed.signature,
    verified: true,
    agent: agentFlag ?? looksLikeAgent(signed.handle),
  };
  return { ok: true, message };
}

function getMessages(slug) {
  const all = readJSON(LS.messages, {});
  return all[slug] || [];
}
function appendMessage(slug, msg) {
  const all = readJSON(LS.messages, {});
  all[slug] = all[slug] || [];
  all[slug].push(msg);
  writeJSON(LS.messages, all);
}

// ---- optional live-node federation ----
// Default the live node to the deployed Cloud Run server so the public Pages
// site uses the live meta-llm out-of-the-box (key stays server-side; the server
// enforces CORS + per-session rate-limit + a daily budget cap). First visit
// (key absent) → live; explicit Disconnect stores '' → local-only demo.
const DEFAULT_LIVE_NODE = 'https://agentbbs-web-63rzcdswba-uc.a.run.app';
export function liveNode() {
  const v = localStorage.getItem(LS.node);
  if (v !== null) return v; // explicit choice (incl. '' = Disconnected → local)
  // Default to the live node ONLY on the public Pages site; dev/test/localhost
  // stay local so the static E2E + offline demo are deterministic.
  try { if (location.hostname.endsWith('github.io')) return DEFAULT_LIVE_NODE; } catch (_) {}
  return '';
}
export function setLiveNode(url) {
  if (url) localStorage.setItem(LS.node, url.replace(/\/+$/, ''));
  else localStorage.removeItem(LS.node);
}

async function fetchLiveBoard(slug) {
  const base = liveNode();
  if (!base) return null;
  try {
    const r = await fetch(base + '/api/boards/' + encodeURIComponent(slug),
      { headers: { 'content-type': 'application/json' } });
    if (!r.ok) return null;
    const data = await r.json();
    return (data.messages || []).map(m => ({
      id: m.id,
      board: slug,
      subject: m.subject,
      body: m.body,
      author: m.author || '',
      short: (m.author || '').slice(0, 8),
      handle: m.handle,
      created_at: m.at,
      verified: m.verified !== false,
      agent: m.agent ?? looksLikeAgent(m.handle),
      remote: true,
    }));
  } catch (_) { return null; }
}

async function pushLive(signed) {
  const base = liveNode();
  if (!base) return false;
  try {
    const r = await fetch(base + '/api/boards/' + encodeURIComponent(signed.board) + '/signed',
      { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(signed) });
    return r.ok;
  } catch (_) { return false; }
}

// ---- public API (shape mirrors the server's HTTP API) ----
export const store = {
  async boot() {
    ensureSeeded();
    const identity = await BBS.loadOrRegister();
    logEvent('node.boot', 'genesis node started · anon @' + identity.id.slice(0, 8));
    return identity;
  },

  boards() {
    const boards = readJSON(LS.boards, SEED_BOARDS);
    return boards.map(b => ({ ...b, count: getMessages(b.slug).length }));
  },

  state() {
    const boards = this.boards();
    const total = boards.reduce((n, b) => n + b.count, 0);
    return { node: PROTOCOL_VERSION + ' · genesis', boards, total_messages: total };
  },

  // Read a board's messages, merging the live node's thread if connected.
  async board(slug) {
    const boards = readJSON(LS.boards, SEED_BOARDS);
    const meta = boards.find(b => b.slug === slug) || { slug, title: slug, description: '' };
    let messages = getMessages(slug);
    // DMs (dm:* slugs) are private: never fetch/merge them from a live node
    // (Phase 1 is local-only; E2E federation is ADR-0037 phase 3).
    const live = slug.startsWith('dm:') ? null : await fetchLiveBoard(slug);
    if (live) {
      // The live node is AUTHORITATIVE when connected. We dedupe by CONTENT
      // (author|created_at|body), not id, because a message's content-addressed
      // id can differ between this browser and the server, which otherwise shows
      // every message twice. We keep only very-recent (<10s) local posts the
      // server hasn't echoed yet, as an optimistic display; once the server
      // returns them (same content key) the local copy drops out.
      // Normalize the timestamp to epoch-ms so JS ("…Z", ms) and Rust
      // ("…+00:00", ns) renderings of the same instant produce the same key.
      const ckey = (m) => (m.author || '') + '|' + Date.parse(m.created_at || '') + '|' + (m.body || '');
      const liveKeys = new Set(live.map(ckey));
      const cutoff = Date.now() - 10000;
      const optimistic = messages.filter(m =>
        !liveKeys.has(ckey(m)) && new Date(m.created_at || 0).getTime() > cutoff);
      messages = [...live, ...optimistic].sort((a, b) =>
        (a.created_at || '').localeCompare(b.created_at || ''));
    }
    return { slug: meta.slug, title: meta.title, description: meta.description, messages };
  },

  // Post a human message: sign in-browser, verify locally, store. The agent
  // reply is generated separately via reply() so the UI can show the human
  // message immediately and a "thinking" indicator while the model responds.
  // Returns { ok, error }.
  async post(seedHex, { board, body, handle = 'you', parent = null }) {
    // Post-path injection guard (ADR-0046) — mirrors agentbbs_core::postguard.
    // Blocks obvious prompt-injection before any @mentioned agent reads it.
    const g = scanPost(body);
    if (g.level === 'malicious') {
      logEvent('post.blocked', g.reasons.join('; '), 'Warn');
      return { ok: false, error: 'post blocked: ' + g.reasons.join('; ') };
    }
    const built = await buildVerifiedMessage(seedHex, { board, body, handle, parent });
    if (!built.ok) {
      logEvent('post.rejected', built.error, 'Warn');
      return { ok: false, error: built.error };
    }
    appendMessage(board, built.message);
    logEvent('post.signed', `@${built.message.short} → #${board}`);

    // Best-effort federation to a live node (non-fatal). DMs (dm:* slugs) are
    // private and NEVER pushed as plaintext (ADR-0037 phase 1 = local-only).
    if (!board.startsWith('dm:')) {
      const signed = {
        board, parent: built.message.parent || null, subject: built.message.subject, body: built.message.body,
        author: built.message.author, handle: built.message.handle,
        created_at: built.message.created_at, signature: built.message.signature,
      };
      pushLive(signed).then(ok => { if (ok) logEvent('federation.push', `replicated to live node`); });
    }
    return { ok: true };
  },

  // Edit/delete via signed control messages (author-only, append-only-safe).
  // A retraction/revision is a normal Ed25519-signed message whose subject marks
  // it as control and whose body references the target id; the renderer hides the
  // retracted message (and the control message) only when the control message's
  // author matches the target's author. The original stays in the log.
  async retract(seedHex, board, targetId) {
    return this._control(seedHex, board, 'agentbbs/ctl:retract:' + targetId, targetId);
  },
  async editPost(seedHex, board, targetId, newText) {
    // subject carries the target id; body carries the new text.
    return this._control(seedHex, board, 'agentbbs/ctl:edit:' + targetId, newText);
  },
  async _control(seedHex, board, subject, body) {
    const built = await buildVerifiedMessage(seedHex, { board, body, handle: 'you', subject });
    if (!built.ok) return { ok: false, error: built.error };
    appendMessage(board, built.message);
    logEvent('post.control', `${subject.replace(/ /, '')} → #${board}`);
    if (!board.startsWith('dm:')) {
      await pushLive({
        board, parent: null, subject: built.message.subject, body: built.message.body,
        author: built.message.author, handle: built.message.handle,
        created_at: built.message.created_at, signature: built.message.signature,
      });
    }
    return { ok: true, author: built.message.author };
  },

  // Generate a signed agent reply to a human post and store it locally.
  // In DEMO mode (replyEngine set) every message gets a semantic, embedding-
  // matched persona reply. Without the engine we fall back to the scripted
  // path, which only fires on an explicit @mention. Returns the reply message
  // (or null if no reply was produced). Never replies to an agent's own post.
  async reply(board, body, handle = 'you') {
    // Don't reply when a live node is driving the thread — the node answers.
    if (liveNode()) return null;
    const mention = detectMention(body);

    let agent, replyBody, subject, agentFlag;
    if (replyEngine) {
      try {
        const r = await replyEngine(body, { mention });
        if (!r) return null;
        agent = r.handle; replyBody = r.body; subject = r.subject || `looped in ${r.handle}`;
        agentFlag = true; // persona replies always render as agents
      } catch (_) { return null; }
    } else if (mention && mention !== (handle || '').toLowerCase()) {
      agent = mention;
      const scripted = composeReply(mention, body);
      replyBody = scripted.body; subject = scripted.subject; agentFlag = looksLikeAgent(mention);
    } else {
      return null;
    }
    if (agent === (handle || '').toLowerCase()) return null;

    const aid = await agentIdentity(agent);
    const abuilt = await buildVerifiedMessage(aid.seed,
      { board, body: replyBody, handle: agent, subject, agentFlag });
    if (!abuilt.ok) return null;
    appendMessage(board, abuilt.message);
    logEvent('agent.loopin', `@${agent} replied in #${board}`);
    return abuilt.message;
  },

  arena() { return readJSON(LS.arena, SEED_ARENA); },
  retort() { return readJSON(LS.retort, SEED_RETORT); },
  // Pod control plane (ADR-0035): spawned pods + Pareto-ranked configs.
  pods() {
    const spawned = readJSON(LS.spawnedPods, []);
    return { pods: [...spawned, ...SEED_PODS], configs: rankPodConfigs(SEED_POD_RESULTS) };
  },
  // Spawn a domain pod with a chosen tier (ADR-0035). On the live server this
  // forwards to /v1/pods/spawn; here it records the intent locally for the demo.
  spawnPod(domain = 'research', tier = 'mid') {
    const caps = { low: 0.05, mid: 0.25, high: 1.0 };
    const spawned = readJSON(LS.spawnedPods, []);
    const id = 'pod-' + Math.floor(performance.now()).toString(36) + spawned.length;
    const pod = { id, domain, host: 'claude-code', tier, status: 'spawned', per_agent_cap_usd: caps[tier] || 0.25, registered_room: `${domain}-ops` };
    spawned.unshift(pod); writeJSON(LS.spawnedPods, spawned);
    logEvent('pod.spawn', `spawned ${id} [${tier}] ${domain} (#${pod.registered_room})`);
    return { ok: true, pod };
  },
  // Decision records (ADR-0045): the org's signed decision memory.
  decisions() {
    const seed = [
      { id: 'dr-meta-llm@1', title: 'Adopt the meta-llm gateway', decision: 'Route live inference through cognitum-auto (ADR-0034)', rationale: 'tier routing + metering + budget caps; OpenRouter stays the default', board: 'agents.dev', decided_by: 'org', decided_at: '2026-06-30T03:00:00Z' },
      { id: 'dr-approval-spend@1', title: 'Human approval for spend', decision: 'All side-effectful spend requires a signed approval (ADR-0038)', rationale: 'fail-closed governance is required to trust the autopilot', board: 'general', decided_by: 'org', decided_at: '2026-06-30T04:00:00Z' },
    ];
    let recorded = [];
    try { recorded = JSON.parse(localStorage.getItem('agentbbs.decisions') || '[]'); } catch (_) { recorded = []; }
    return { decisions: [...recorded, ...seed] };
  },
  // Record a new decision (ADR-0045), Ed25519-signed in-browser; stored locally.
  async recordDecision(seedHex, { title, decision, rationale, board = 'general' }) {
    if (!title || !decision) return { ok: false, error: 'title and decision are required' };
    const signed = await BBS.signPost(seedHex, { board: 'decisions', body: `${title}\n${decision}\n${rationale || ''}`, handle: 'you' });
    const rec = { id: signed.signature.slice(0, 16), title, decision, rationale: rationale || '', board, decided_by: signed.author.slice(0, 8), decided_at: signed.created_at };
    let all = []; try { all = JSON.parse(localStorage.getItem('agentbbs.decisions') || '[]'); } catch (_) {}
    all.unshift(rec); localStorage.setItem('agentbbs.decisions', JSON.stringify(all.slice(0, 50)));
    return { ok: true, rec };
  },

  // Playbooks (ADR-0041): versioned workflow definitions composing agent tasks,
  // approval gates, and tools.
  playbooks() {
    return {
      playbooks: [{
        playbook_id: 'triage-inbound-lead@1', name: 'triage-inbound-lead', version: '1', trigger: 'event:lead.created',
        steps: [
          { id: 'research', kind: 'agent_task', agent: 'claude', instruction: 'enrich the lead from public sources' },
          { id: 'approve-spend', kind: 'approval_gate', summary: 'approve $5 enrichment spend' },
          { id: 'crm', kind: 'tool', tool: 'crm.upsert' },
        ],
      }, {
        playbook_id: 'nightly-security-audit@1', name: 'nightly-security-audit', version: '1', trigger: 'cron:0 3 * * *',
        steps: [
          { id: 'scan', kind: 'agent_task', agent: 'graybeard', instruction: 'enumerate deps + match CVE feeds' },
          { id: 'repro', kind: 'tool', tool: 'sandbox.exec' },
          { id: 'sign-off', kind: 'approval_gate', summary: 'approve filing HIGH/CRITICAL findings' },
        ],
      }],
    };
  },

  // Budget guardrails (ADR-0040): per-pod spend vs cap, over-budget flagged.
  budget() {
    const { pods } = this.pods();
    return {
      budgets: pods.map(p => {
        const spent = SEED_POD_SPEND[p.id] || 0;
        const cap = p.per_agent_cap_usd || 0;
        return { pod_id: p.id, domain: p.domain, spent, cap, remaining: Math.max(0, cap - spent), over_budget: cap > 0 && spent >= cap, pct: cap > 0 ? spent / cap : 0 };
      }),
    };
  },

  // "Hire the winner" (ADR-0035 + ADR-0039): spawn a pod with the chosen agent
  // as its host. Local in the demo; the live path spawns via the cog_ gateway.
  hire(handle, domain = 'ops') {
    handle = (handle || '').trim().toLowerCase().replace(/^@/, '');
    if (!handle) return { ok: false, error: 'no agent' };
    const spawned = readJSON(LS.spawnedPods, []);
    const id = 'pod-hire-' + spawned.length;
    const pod = { id, domain, host: handle, tier: 'mid', status: 'spawned', per_agent_cap_usd: 0.25, registered_room: `${domain}-ops` };
    spawned.unshift(pod);
    writeJSON(LS.spawnedPods, spawned);
    logEvent('pod.hire', `hired @${handle} → ${id} (#${pod.registered_room})`);
    return { ok: true, pod };
  },

  // Agent directory (ADR-0039): agents ranked by confidence-adjusted reputation.
  directory() {
    const agents = SEED_AGENT_RECORDS.map(r => {
      const rate = r.total ? r.successes / r.total : 0;
      return { handle: r.handle, kind: r.kind, successes: r.successes, total: r.total, rate, score: wilsonLB(r.successes, r.total), credentials: r.credentials || [], status: r.status || 'active' };
    }).sort((a, b) => b.score - a.score || b.total - a.total || a.handle.localeCompare(b.handle))
      .map((a, i) => ({ ...a, rank: i + 1 }));
    return { agents };
  },

  // Approval gates (ADR-0038): pending proposals + their authorization state.
  // A decision is an Ed25519-signed message; authorized iff approved and not
  // vetoed (fail-closed). Local-only in the genesis demo.
  proposals() {
    const decisions = readJSON(LS.approvalDecisions, []);
    return {
      proposals: SEED_PROPOSALS.map(p => {
        const ds = decisions.filter(d => d.action_id === p.action_id);
        const rejected = ds.some(d => d.verdict === 'reject');
        const approved = ds.some(d => d.verdict === 'approve');
        return { ...p, decisions: ds, authorized: approved && !rejected };
      }),
    };
  },
  // Sign a human decision in-browser and record it. Returns { ok, error }.
  async decide(seedHex, actionId, verdict, reason = '') {
    const built = await buildVerifiedMessage(seedHex, {
      board: 'approvals', handle: 'you', subject: verdict,
      body: `${verdict}:${actionId}:${reason}`,
    });
    if (!built.ok) return { ok: false, error: built.error };
    const decisions = readJSON(LS.approvalDecisions, []);
    decisions.push({ action_id: actionId, verdict, reason, decider: built.message.short, signature: built.message.signature });
    writeJSON(LS.approvalDecisions, decisions);
    logEvent('approval.decision', `${verdict} → ${actionId}`);
    return { ok: true };
  },
  market() { return { listings: readJSON(LS.market, SEED_MARKET) }; },
  doors() { return { doors: DOORS }; },
  federation() { return { ...FEDERATION, mode: liveNode() ? 'federated' : 'demo', peers: liveNode() ? [{ addr: liveNode() }] : [] }; },

  // Who's online: distinct recent message authors/handles across all boards.
  online(me) {
    const boards = readJSON(LS.boards, SEED_BOARDS);
    const seen = new Set();
    const online = [];
    for (const b of boards) {
      const msgs = getMessages(b.slug).slice(-50);
      for (const m of msgs) {
        const handle = m.handle || m.short;
        if (!seen.has(handle)) {
          seen.add(handle);
          online.push({
            handle,
            kind: looksLikeAgent(handle) ? 'agent' : 'human',
            action: `active in #${b.slug}`,
          });
        }
      }
    }
    return { sessions: 1, you: me || '', online };
  },

  // Sysop report: the in-memory event log of posts/board events.
  report() {
    return { count: sysopEvents.length, events: sysopEvents.slice(0, 40) };
  },
};
