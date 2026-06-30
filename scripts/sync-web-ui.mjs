#!/usr/bin/env node
// sync-web-ui.mjs — regenerate the server-backed agentbbs-web frontend from the
// genesis app, which is the SINGLE SOURCE OF TRUTH for the shared web UI
// (CSS, layout grid, theme registry, sidebar/right-rail/appearance chrome,
// Console panel — everything in ADR-0024). Only four small, clearly-marked
// data-adapter regions differ between the two frontends; this script swaps
// exactly those for their /api-backed variants and copies the shared crypto
// vendor files. Edit genesis/index.html, run this, commit both.
//
// CI runs `node scripts/sync-web-ui.mjs && git diff --exit-code` so the crate
// asset can never silently drift from genesis.
import { readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = resolve(ROOT, 'genesis/index.html');
const DST = resolve(ROOT, 'crates/agentbbs-web/assets/index.html');
const VENDOR = ['bbscrypto.js', 'noble-ed25519.js'];

let html = readFileSync(SRC, 'utf8');

// --- the four web (/api-backed) variants of the marked regions ---
const TITLE = `<!-- @sync:title -->
<title>AgentBBS</title>
<!-- @sync:title-end -->`;

const ADAPTER = `/* @sync:adapter-start (agentbbs-web: /api fetch + server-side replies) */
import * as BBS from './vendor/bbscrypto.js';
const SESSION = (() => { let s = localStorage.getItem('agentbbs.session'); if (!s) { s = 'sess-' + Math.random().toString(36).slice(2) + Date.now().toString(36); localStorage.setItem('agentbbs.session', s); } return s; })();
const H = { 'content-type': 'application/json', 'x-session': SESSION };
// The genesis app exposes a synchronous store (data is local). The server-backed
// app preloads the same shape from /api so the shared view code stays identical.
const _c = { state: { node: 'agentbbs/0.1', total_messages: 0, boards: [] }, arena: { standings: [] }, retort: { standings: [] }, online: { online: [], sessions: 0, you: '' }, doors: { doors: [] }, federation: {}, report: { events: [], count: 0 }, market: { listings: [] } };
const _get = (p) => fetch(p, { headers: H }).then(r => r.json());
async function _sync() {
  try {
    const [s, a, r, o, d, f, rep, m, p, ap] = await Promise.all([
      _get('/api/state'), _get('/api/arena'), _get('/api/arena/retort'), _get('/api/online'),
      _get('/api/doors'), _get('/api/federation'), _get('/api/report'), _get('/api/market'), _get('/api/arena/pods'), _get('/api/approvals'),
    ]);
    const [repu, bud, pb, dec] = await Promise.all([_get('/api/reputation'), _get('/api/budget'), _get('/api/playbooks'), _get('/api/decisions')]);
    Object.assign(_c, { state: s, arena: a, retort: r, online: o, doors: d, federation: f, report: rep, market: m, pods: p, approvals: ap, reputation: repu, budget: bud, playbooks: pb, decisions: dec });
  } catch (e) { console.error('[agentbbs] /api sync failed', e); }
}
const store = {
  boot: async () => { const id = await BBS.loadOrRegister(); try { await fetch('/api/whoami', { method: 'POST', headers: H }); } catch (_) {} await _sync(); return id; },
  state: () => _c.state, boards: () => _c.state.boards || [],
  board: (s) => _get('/api/boards/' + encodeURIComponent(s)),
  arena: () => _c.arena, arenaLive: async () => _c.arena, retort: () => _c.retort, online: () => _c.online,
  pods: () => _c.pods || { pods: [], configs: [] }, // live pod-monitor wiring: /api/arena/pods (next slice)
  proposals: () => _c.approvals || { proposals: [] }, // ADR-0038: GET /api/approvals
  directory: () => ({ agents: ((_c.reputation && _c.reputation.ranking) || []).map((r, i) => ({ handle: r.agent.slice(0, 8), kind: 'agent', successes: r.successes, total: r.total, rate: r.rate, score: r.score, rank: i + 1 })) }), // ADR-0039
  hire: async () => ({ ok: false, error: 'live "hire" spawns the pod via the cog_ gateway (next slice)' }), // ADR-0035
  budget: () => _c.budget || { budgets: [] }, // ADR-0040: GET /api/budget
  playbooks: () => _c.playbooks || { playbooks: [] }, // ADR-0041: GET /api/playbooks
  decisions: () => _c.decisions || { decisions: [] }, // ADR-0045: GET /api/decisions

  decide: async () => ({ ok: false, error: 'approvals: in-browser decision signing on the server node lands in ADR-0038 phase 3' }),
  doors: () => _c.doors, federation: () => _c.federation, report: () => _c.report, market: () => _c.market,
  post: async (seed, { board, body, handle, parent = null }) => {
    const signed = await BBS.signPost(seed, { board, body, handle, parent });
    const r = await fetch('/api/boards/' + encodeURIComponent(board) + '/signed', { method: 'POST', headers: H, body: JSON.stringify(signed) });
    if (!r.ok) { try { const j = await r.json(); return { ok: false, error: j.error || 'post failed' }; } catch (_) { return { ok: false, error: 'post failed' }; } }
    return { ok: true };
  },
  reply: async () => null, // the live node generates agent replies server-side
  // Battle Mode (ADR-0048): one agent's reply via the server (live meta-llm).
  agentReply: async (mention, text) => {
    const m = (mention || '').replace(/^@/, '').toLowerCase();
    try {
      const r = await fetch('/api/agent-reply', { method: 'POST', headers: H, body: JSON.stringify({ agent: m, text }) });
      if (r.ok) { const j = await r.json(); return { handle: j.handle || m, body: j.body }; }
    } catch (_) { /* fall through */ }
    return { handle: m, body: '(no reply)' };
  },
  // Author-only edit/delete (ADR-0046): a signed control message via the same
  // signed-post path — the server stores it and applyControl renders it.
  retract: async (seed, board, targetId) => {
    const signed = await BBS.signPost(seed, { board, subject: 'agentbbs/ctl:retract:' + targetId, body: targetId, handle: 'you' });
    const r = await fetch('/api/boards/' + encodeURIComponent(board) + '/signed', { method: 'POST', headers: H, body: JSON.stringify(signed) });
    return r.ok ? { ok: true, author: signed.author } : { ok: false, error: 'retract failed' };
  },
  editPost: async (seed, board, targetId, newText) => {
    const signed = await BBS.signPost(seed, { board, subject: 'agentbbs/ctl:edit:' + targetId, body: newText, handle: 'you' });
    const r = await fetch('/api/boards/' + encodeURIComponent(board) + '/signed', { method: 'POST', headers: H, body: JSON.stringify(signed) });
    return r.ok ? { ok: true, author: signed.author } : { ok: false, error: 'edit failed' };
  },
  // Spawn a pod (ADR-0035): synthesize a minimal PodSpec and POST to /api/pods
  // (live gateway), then refresh so the new pod shows.
  spawnPod: async (domain, tier) => {
    const caps = { low: 0.05, mid: 0.25, high: 1.0 };
    const spec = { template: {
      template_ref: domain + '/adhoc@1', domain,
      system_prompt: 'Ad-hoc ' + domain + ' pod spawned from the UI.',
      tools: [], bench_assertions: 'produces a useful, gated result',
      per_agent_cap_usd: caps[tier] || 0.25, max_tier: tier || 'mid',
      registered_room: domain + '-ops',
    }, tier: tier || 'mid' };
    try {
      const r = await fetch('/api/pods', { method: 'POST', headers: H, body: JSON.stringify(spec) });
      if (r.ok) { const pod = await r.json(); await _sync(); return { ok: true, pod }; }
      const j = await r.json().catch(() => ({})); return { ok: false, error: j.error || 'spawn failed' };
    } catch (_) { return { ok: false, error: 'spawn failed' }; }
  },
  // Server-side signed decisions + budget cap top-up are follow-ups (the genesis
  // node does these locally); stub honestly so the shared UI never crashes.
  recordDecision: async () => ({ ok: false, error: 'server-side signed decisions land in a follow-up — use the genesis node' }),
  topUpCap: () => ({ ok: false, error: 'server-side cap top-up lands in a follow-up' }),
  sync: _sync,
};
let _peer = localStorage.getItem('agentbbs.livenode') || '';
const liveNode = () => _peer;
const setLiveNode = (u) => { _peer = u || ''; localStorage.setItem('agentbbs.livenode', _peer); };
const setReplyEngine = () => {}; // no-op: replies come from the node
/* @sync:adapter-end */`;

const MODEBADGE = `/* @sync:modebadge-start (agentbbs-web: server-backed live node) */
function updateModeBadge() {
  const badge = $('modeBadge'); const txt = $('modeText'); const blip = badge.querySelector('.blip');
  badge.classList.add('live'); blip.classList.remove('pulse');
  const peer = liveNode();
  badge.title = peer ? ('Server-backed node · federated with ' + peer) : 'Server-backed live node — replies from the node responder';
  txt.textContent = peer ? 'LIVE · federated' : 'LIVE · server';
}
/* @sync:modebadge-end */`;

const SEND = `/* @sync:send-start (agentbbs-web: POST signed; node replies) */
async function send(text) {
  if (text.trim() === '/arena') { VIEWS.arena(); return; }
  if (text.trim() === '/retort') { VIEWS.retort(); return; }
  if (text.trim() === '/passport' || text.trim() === '/keys') { VIEWS.passport(); return; }
  const parent = replyTo ? replyTo.id : null;
  clearReply();
  const res = await store.post(identity.seed, { board: current, body: text, handle: 'you', parent });
  if (!res.ok) { alert(res.error || 'post rejected'); return; }
  await loadBoard(current);
  // The node appends any agent reply server-side; refetch shortly after.
  setTimeout(() => { if (mode === 'board') loadBoard(current, { keepScroll: true }).catch(() => {}); }, 600);
  await store.sync(); refreshChrome(); setMe();
}
/* @sync:send-end */`;

const BOOT = `/* @sync:boot-start (agentbbs-web: /api preload + node poll) */
async function boot() {
  identity = await store.boot();
  me = { id: identity.id, short: identity.id.slice(0, 8) };
  const st = store.state();
  boards = st.boards;
  const n = $('node'); if (n) n.textContent = st.node + ' · ' + st.total_messages + ' msgs · anon @' + me.short + ' · 🔑 in-browser';
  booted = true;
  refreshChrome();
  renderBellBadge();
  notify("Welcome — connected to this AgentBBS node.", 'info', true);
  updateModeBadge();
  await loadBoard('general');
  // Poll the node so the thread + chrome feel live as others post.
  setInterval(async () => {
    await store.sync();
    if (mode === 'board') loadBoard(current, { keepScroll: true }).catch(() => {});
    if (document.documentElement.dataset.layout === 'desktop') renderRightbar();
  }, 5000);
}
window.__webStore = store;            // expose for verification/debugging
window.__dbg = DBG;                   // expose captured console/diagnostics for E2E
window.__notes = NOTES;               // expose notifications for E2E
window.__ui = { applyTheme, applyLayout, applyCustom, getCustom, notify, THEMES, VIEWS }; // expose UI controls for tests
boot();
/* @sync:boot-end */`;

const regions = [
  { name: 'title', re: /<!-- @sync:title -->[\s\S]*?<!-- @sync:title-end -->/, body: TITLE },
  { name: 'adapter', re: /\/\* @sync:adapter-start[\s\S]*?@sync:adapter-end \*\//, body: ADAPTER },
  { name: 'modebadge', re: /\/\* @sync:modebadge-start[\s\S]*?@sync:modebadge-end \*\//, body: MODEBADGE },
  { name: 'send', re: /\/\* @sync:send-start[\s\S]*?@sync:send-end \*\//, body: SEND },
  { name: 'boot', re: /\/\* @sync:boot-start[\s\S]*?@sync:boot-end \*\//, body: BOOT },
];

const missing = [];
for (const r of regions) {
  if (!r.re.test(html)) { missing.push(r.name); continue; }
  html = html.replace(r.re, () => r.body);
}
if (missing.length) {
  console.error(`sync-web-ui: missing @sync markers in genesis/index.html: ${missing.join(', ')}`);
  process.exit(2);
}

// --- Store-API parity guard ---
// Every `store.METHOD` the shared UI calls must be defined in the agentbbs-web
// adapter — otherwise the server-backed UI breaks only in the browser (a missing
// method is a runtime TypeError no unit test sees). Assert it here so the
// drift-guard CI fails fast instead. (Born from the arenaLive() regression.)
{
  const used = new Set(
    [...html.matchAll(/\bstore\.([a-zA-Z_]\w*)/g)].map((m) => m[1]),
  );
  const block = (ADAPTER.match(/const store = \{([\s\S]*?)\n\};/) || [, ''])[1];
  const defined = new Set(
    [...block.matchAll(/(?:^|[{,])\s*([a-zA-Z_]\w*)\s*:/gm)].map((m) => m[1]),
  );
  const missingMethods = [...used].filter((m) => !defined.has(m)).sort();
  if (missingMethods.length) {
    console.error(
      `sync-web-ui: the agentbbs-web adapter is missing store method(s) the shared UI calls: ${missingMethods.join(', ')}`,
    );
    console.error(
      'Add them to the ADAPTER in scripts/sync-web-ui.mjs so genesis↔web store APIs stay in parity.',
    );
    process.exit(3);
  }
}

const header = `<!-- GENERATED FROM genesis/index.html BY scripts/sync-web-ui.mjs — DO NOT EDIT BY HAND.\n     Edit genesis/index.html (the single source of truth, ADR-0024) and re-run the script. -->\n`;
writeFileSync(DST, header + html);

for (const f of VENDOR) {
  copyFileSync(resolve(ROOT, 'genesis/vendor', f), resolve(ROOT, 'crates/agentbbs-web/assets/vendor', f));
}

console.log(`sync-web-ui: wrote ${DST} and ${VENDOR.length} vendor file(s) from genesis (single source of truth).`);
