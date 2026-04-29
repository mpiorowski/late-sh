# PLAN — Vanilla Web-Browser Spectator Viewer for late.sh (POC1)

## Goal

Ship a minimal browser-only "demo mode" terminal viewer for late.sh. A user
hits a same-origin URL on `late-web`, an HTML page with `xterm.js` opens,
a WebSocket connects back to `late-web`, and the browser sees the live
late.sh TUI as if they had `ssh`'d in — except read-only and main-screen
only. Each browser visit gets its own backend session; this is **demo mode**,
not spectate-an-existing-user. POC1 lays the protocol/proxy foundation;
future work layers audio + per-session interactivity on top.

## Decisions locked in (Phase 0)

- **Topology:** browser → NGINX/Caddy → `late-web:3000` (HTML page + WS
  endpoint, same origin) → cluster-internal `service-ssh-internal:4001/tunnel`.
- **Mode:** demo. Each browser visit spawns its own fresh backend TUI session.
- **Anonymous credential:** single shared anonymous account (one DB row,
  fixed fingerprint + username). Future work will move to per-session temp
  accounts with a harvest job; not in POC1's scope.
- **Initial cols/rows:** browser sends `?cols=&rows=` query params on the
  WS URL (computed from FitAddon before connect). Late-web reads them and
  forwards as `X-Late-Cols`/`X-Late-Rows` on the upstream handshake.
- **Page URL:** `/spectate`.
- **View-only enforcement:** in `late-ssh`, **not** at the proxy. Late-web
  forwards all bytes both directions verbatim. Late-ssh gates state-mutating
  action dispatches on a `view_only` flag set by a new handshake header.
  Bytes still flow into the vte parser, so terminal-query responses (DA, DSR,
  cursor-position reports, etc.) are not silently dropped — important if any
  future TUI starts interrogating the terminal. Browser-side
  `disableStdin: true` + `attachCustomKeyEventHandler(() => false)` is UX
  hygiene only.
- **Verification target:** l8.st preview (`~/p/my/l8-infra/`). No `late.sh`
  rollout in POC1.
- **Preview env wiring:** secrets via
  `~/p/my/l8-infra/scripts/set-secrets.sh`, mirroring the existing
  `LATE_TUNNEL_SHARED_SECRET` pattern.

## Key findings (research summary)

### `/tunnel` wire protocol is reusable verbatim

`late-ssh/src/tunnel.rs` + `late-ssh/src/session_io.rs`:

- Server → client: `Message::Binary(Vec<u8>)` per ratatui frame; raw VT
  bytes. Initial `\x1b[?1049h` (alt-screen-enter) explicitly pushed before
  the render loop starts (`tunnel.rs:486–490`).
- Client → server: `Message::Binary` for keystrokes,
  `Message::Text` for `ControlFrame::Resize { cols, rows }` JSON.
- Close codes: `1000` (server drain — retryable), `4000` (session ended —
  terminal). Tungstenite layer auto-pongs.
- Backpressure: 50ms send timeout per frame (`session_io.rs:23`); drops
  surface as `Ok(false)` and trigger a full repaint.
- Ordering: single `mpsc<SshInputEvent>` (`tunnel.rs:430`) preserves
  Bytes/Resize ordering — critical for mouse/paste/resize cohabitation.

### `/tunnel` is bastion-only by handshake design

Validated handshake (`tunnel.rs::validate_handshake`):

- `X-Late-Secret` (PSK)
- CIDR allowlist on transport peer IP (`LATE_TUNNEL_TRUSTED_CIDRS`)
- Required: `X-Late-Fingerprint`, `X-Late-Username`, `X-Late-Peer-IP`,
  `X-Late-Term`, `X-Late-Cols`, `X-Late-Rows`. Optional: `X-Late-Reconnect`,
  `X-Late-Session-Id`.
- Backend resolves the user via `ensure_user(state, username, fingerprint)`
  (`ssh.rs:991`): finds-or-creates by fingerprint, auto-joins public chat
  rooms on first creation.

late-web becomes a second trusted client of `/tunnel`, asserting synthetic
anon credentials over the cluster-internal network.

### `late-web` already serves static + askama HTML; mirrors `connect/`

- Routes via `pages::router()` merging per-page subrouters
  (`late-web/src/pages/mod.rs`).
- Static assets at `late-web/static/` via `ServeDir` (`lib.rs:26`).
- `pages/connect/` is the closest analog: page handler renders askama
  template, template references `/static/…` and an external WS endpoint.
- `AppState { config, db, http_client: reqwest::Client }`.

### xterm.js: defaults sufficient for ratatui

(`@xterm/xterm` v6.0.0, MIT, `~/p/gh/xterm.js`)

- `term.write(Uint8Array)` accepts raw UTF-8 PTY bytes; parser handles
  split escape sequences across writes.
- 256-color, truecolor, alt-screen, box-drawing all work out of the box.
  Default canvas renderer fine for POC1.
- Read-only browser hygiene: `disableStdin: true` plus
  `attachCustomKeyEventHandler(() => false)`.
- `@xterm/addon-fit` for container-sized terminals.
- VS Code's custom resize debouncer + theme-from-config wiring is overkill
  here.

### Render loop survives no-input sessions

`WORLD_TICK_INTERVAL = 66ms` (`ssh.rs:39`) — render loop unconditionally
ticks at ~15fps. External state changes (chat, now-playing, activity feed)
poke `signal.dirty` directly, so the spectator's view stays current
without needing keystroke wakes.

### Infra reuse

- **Preview (`l8.st`, Caddy):** `reverse_proxy` upgrades WS transparently
  — no Caddyfile change.
- **Prod (`late.sh`, NGINX/k8s):** `service-web-ingress` lacks the
  WS-friendly annotations that `service-ssh-api-ingress` has — TF edit
  required (deferred until prod rollout, not POC1).
- `service-ssh-internal-sv` ClusterIP exposes `:4001/tunnel` to the bastion
  pod CIDR. Late-web's pod CIDR must be added to
  `LATE_TUNNEL_TRUSTED_CIDRS` (or a sibling), and late-web needs
  `LATE_TUNNEL_SHARED_SECRET`.

## Approach

**Code-reuse plan:**

- Lift `late_bastion::handshake::build_request` (and `HandshakeContext`)
  into `late_core::tunnel_protocol`. Header constants already live there.
  Both bastion and late-web depend on it.
- Add `HEADER_VIEW_ONLY` constant to `late_core::tunnel_protocol`. Optional
  handshake header; absence/`"0"` = full-input session, `"1"` = view-only.
- Plumb `view_only: bool` through `TunnelHandshake` →
  `SessionBootstrapInputs` → `SessionConfig` → `App`.
- New `late-web/src/pages/spectate/` module: page handler + WS handler.
  WS handler does `connect_async` + bidirectional byte-pump pattern modeled
  on `late-bastion/src/proxy.rs`, **stripped of**:
  - russh side (replaced with the browser WS).
  - reconnect logic (POC1 lets backend disconnects propagate to the
    browser as WS Close).
  - "reconnecting…" plain-text injection (only useful when there's a
    persistent SSH session to keep alive).
- Late-web forwards all bytes both directions verbatim. Read-only is
  enforced backend-side (see "view-only enforcement" decision).

**Conventions:**

- Error handling: `late-web` uses `anyhow::Error` flowing into
  `error::AppError` (`late-web/src/error.rs`).
- Logging: `tracing` spans named `web.spectate.handshake`,
  `web.spectate.upstream`, `web.spectate.client` — parallel with
  `tunnel.rs` for cross-service trace correlation.
- Vendor xterm.js + addon-fit under `late-web/static/vendor/xterm/`
  (deterministic, offline-friendly, matches existing `static/` shape).
  No CDN at runtime.
- LLM agents do NOT run `cargo test`/`clippy`/`nextest` per CONTEXT.md
  §Test Strategy. Note expected commands in PR/handoff.
- Integration tests under `late-web/tests/` use a fake `/tunnel` axum
  test server, not a live `late-ssh`.

## Task breakdown (MVP slice)

Each step is small enough to land as one PR or one commit. Order matters:
earlier steps unblock later ones.

### Phase A — protocol scaffolding

1. **Lift `build_request` + `HandshakeContext` into
   `late_core::tunnel_protocol`.**
   - Move from `late-bastion/src/handshake.rs` (already pure-logic, has
     unit tests) into `late-core/src/tunnel_protocol.rs` (or sibling
     module under it). Migrate tests.
   - Update `late-bastion` to import from `late_core::tunnel_protocol`.
   - No behavior change.

2. **Add `HEADER_VIEW_ONLY` and `view_only` field to the handshake.**
   - In `late_core::tunnel_protocol`: define
     `HEADER_VIEW_ONLY = "x-late-view-only"`.
   - In `late_ssh::tunnel::TunnelHandshake`: add `pub view_only: bool`,
     populate in `validate_handshake` (`Some("1")` → true, else false).
   - In `late_bastion::handshake::HandshakeContext` + `build_request`: add
     `view_only: bool`; emit header when `true`. Bastion always sets
     `false` (no behavior change for the bastion path).
   - Unit tests for each end (round-trip presence/absence).

### Phase B — backend view-only enforcement

3. **Plumb `view_only` through `SessionConfig` → `App`.**
   - `late-ssh/src/session_bootstrap.rs::SessionBootstrapInputs`: add
     `view_only: bool`. Default `false`. Forward into `SessionConfig`.
   - `late-ssh/src/app/state.rs::SessionConfig` + `App`: add
     `view_only: bool` field; wire through `App::new`.

4. **Gate state-mutating action dispatch on `!view_only` in
   `late-ssh/src/app/input.rs`.**
   - Top of `handle_parsed_input` (or `handle_byte_event`,
     `handle_overlay_input`, etc. — implementer's call): early-return
     when `app.view_only` and the parsed event would mutate state.
   - **Critical:** bytes still flow through the vte parser
     (`handle_vt_segment`) so the parser state machine stays consistent
     and any future "read terminal-query responses" feature can opt in.
   - Pure-logic tests: feed a known keystroke byte sequence into a
     `view_only=true` `App` and assert no state change; same input on
     `view_only=false` and assert the expected state change.

5. **Suppress activity-feed broadcast and chat auto-join for the
   spectator user.**
   - `late-ssh/src/tunnel.rs:392`: don't broadcast `joined` ActivityEvent
     when the asserted username matches a spectator pattern (e.g.,
     `username == "spectator"` or fingerprint prefix `web-spectator:`).
     Same suppression for any `left` semantics in
     `TunnelSessionGuard::drop`.
   - `late-ssh/src/ssh.rs::ensure_user`: skip
     `chat_service.auto_join_public_rooms` for the spectator pattern.
   - Pure conditional based on the well-known synthetic identity.
     Unit-test the predicate in isolation so refactors can't silently
     widen it.

6. **(Deferred — track in CONTEXT.md follow-ups.) Separate connection
   semaphore for spectators.** Spectators today share
   `state.conn_limit` with real SSH users (`tunnel.rs:315`). A spectator
   surge can starve real users. Either add a sibling
   `state.spectator_conn_limit` selected by user pattern, or oversize the
   global. Skip in POC1; document an interim cap in PR notes.

### Phase C — late-web spectator page + WS proxy

7. **Add `late-web/src/pages/spectate/` module.**
   - `mod.rs`: routes — `GET /spectate` returns the page,
     `GET /ws/spectate` upgrades to WS.
   - `page.html` askama template — extends `pages/app.html`. Contains a
     full-viewport `<div id="terminal">`, loads
     `/static/vendor/xterm/xterm.css`, `/static/vendor/xterm/xterm.js`,
     `/static/vendor/xterm/addon-fit.js`, and `/static/spectate.js`.
   - `spectate.js`: construct `Terminal({disableStdin:true, fontFamily,
     theme})`, load FitAddon, call `fit()` after open, read computed
     `cols`/`rows` from the FitAddon proposed dims, open WS to
     `wss://<host>/ws/spectate?cols=<n>&rows=<m>`.
   - On WS binary: `term.write(new Uint8Array(ev.data))`.
   - On `window.resize` (debounced ~150ms): `fit()` then send
     `JSON.stringify({type:"Resize",cols,rows})` Text frame.
   - On WS close: surface a "session ended" overlay.
   - `attachCustomKeyEventHandler(() => false)` in addition to
     `disableStdin: true`.
   - Vendor xterm.js v6 + addon-fit under
     `late-web/static/vendor/xterm/`.

8. **Add upstream WS dial + bidirectional pump in the WS handler.**
   - `axum::extract::ws::WebSocketUpgrade` for browser-side; on upgrade,
     mint synthetic identity (config-supplied shared
     `spectator_username` + `spectator_fingerprint`), resolve effective
     client IP from `X-Forwarded-For` (crib
     `late-ssh/src/api.rs::effective_client_ip`), parse `cols`/`rows`
     from query params (with sane fallback default + clamp), construct
     `HandshakeContext` with `view_only: true`, call
     `late_core::tunnel_protocol::build_request`.
   - Use `tokio_tungstenite::connect_async` (already in workspace via
     late-bastion).
   - Pump:
     - Upstream Binary → browser Binary.
     - Upstream Text → forward to browser (currently no server-originated
       control frames, but don't drop them — log + forward for forward
       compatibility).
     - Upstream Close(code, reason) → forward Close to browser.
     - Browser Binary → forward upstream verbatim (view-only is enforced
       backend-side).
     - Browser Text → forward upstream verbatim (the only valid value
       today is `Resize`, but pass through unchanged).
     - Browser Close → close upstream.
   - Single bounded mpsc + writer task on each side, mirroring the
     `tunnel.rs` writer pattern, so per-frame backpressure surfaces
     cleanly.

9. **Config wiring in `late-web/src/config.rs`.**
   - `tunnel_url: String` (`LATE_WEB_TUNNEL_URL`,
     e.g. `ws://service-ssh-internal:4001/tunnel`)
   - `tunnel_shared_secret: String` (`LATE_WEB_TUNNEL_SHARED_SECRET`)
   - `spectator_username: String` (default `"spectator"`)
   - `spectator_fingerprint: String` (default `"web-spectator:v1"`)
   - `spectator_default_cols: u16` (default `120`)
   - `spectator_default_rows: u16` (default `40`)
   - `spectator_max_cols: u16` (default `300`) — clamp on browser-supplied
     values.
   - `spectator_max_rows: u16` (default `100`) — same.
   - Update root `Makefile` dev block with sane defaults; reuse
     `LATE_TUNNEL_SHARED_SECRET` already shared with the bastion.

### Phase D — preview infra (l8.st)

10. **Preview compose + secrets** (`~/p/my/l8-infra/`):
    - Add `LATE_WEB_TUNNEL_URL`, `LATE_WEB_TUNNEL_SHARED_SECRET`
      (mirrors prod env var pattern) to
      `compose/docker-compose.preview.yml` for the `service-web` service.
    - Use `scripts/set-secrets.sh` to upload the secret to the GitHub
      `preview` environment, matching how `LATE_TUNNEL_SHARED_SECRET` is
      already managed.
    - No Caddyfile change needed — `reverse_proxy service-web:3000`
      already upgrades WS.

11. **Production infra deferred — track in CONTEXT.md follow-ups.**
    Ship POC1 to l8.st only. Before any `late.sh` rollout:
    - `infra/ingress.tf::service-web-ingress`: add
      `nginx.ingress.kubernetes.io/proxy-read-timeout: 3600`,
      `proxy-send-timeout: 3600`, `proxy-http-version: "1.1"`.
    - `infra/network-policies.tf`: allow
      `service-web` → `service-ssh-internal:4001`.
    - `infra/service-ssh.tf`: extend `LATE_TUNNEL_TRUSTED_CIDRS` to
      include the late-web pod CIDR alongside the bastion's.

### Phase E — manual verification (l8.st)

12. **Local compose smoke:** `make start`, browse
    `http://localhost:3000/spectate`, confirm dashboard renders and
    animates without input. Type keystrokes; confirm nothing dispatches
    backend-side (verify via late-ssh logs / by looking for action
    spans).
13. **Resize the browser** — backend re-paints at the new size within
    ~200ms (FitAddon → Resize text frame → upstream).
14. **Bounce `service-ssh`** — browser shows a clean close (no
    "reconnecting…" overlay; reconnect is deferred).
15. **Confirm activity feed silence** — open and close the spectator page
    repeatedly while watching another (real SSH) session's activity
    sidebar; verify no "spectator joined / left" lines appear.
16. **Deploy to l8.st**, repeat (12)–(15) over `https://l8.st/spectate`.
17. **Trace plumbing check:** confirm late-web's `web.spectate.upstream`
    span shows `late-ssh`'s `tunnel.handle_session` as a child via
    OTel propagation (when `otel` feature is built).

## Testing strategy

Per CONTEXT.md §Test Strategy. Write only what carries weight; the
implementing agent will add more during development.

**Unit tests (inline `#[cfg(test)] mod tests` — pure logic only):**

- `late-core::tunnel_protocol`: `HEADER_VIEW_ONLY` round-trip
  (`build_request` emits header iff `ctx.view_only`, `validate_handshake`
  parses correctly).
- `late-ssh::app::input` (or wherever the action gate lands): a known
  keystroke byte-sequence + `view_only=true` → no state mutation;
  same input + `view_only=false` → expected state mutation.
- Spectator-identity predicate (the function used by §Phase B step 5)
  pinned to its exact pattern.
- `late-web::config` parsing — env var present/missing/malformed for
  each new field, including cols/rows clamps.
- `late-web::pages::spectate` query-param parsing — `?cols=&rows=`
  present/absent/out-of-range.
- Effective-client-IP extraction from `X-Forwarded-For`, mirroring
  `late-ssh/src/api.rs`'s test cases.

**Integration tests (`late-web/tests/spectate/`):**

Spawn a fake `/tunnel` axum server in-process that:
- Validates the `X-Late-Secret`, `X-Late-View-Only: 1`, and asserted
  identity headers.
- On accept, sends a known binary "hello-world VT" frame.
- Echoes received Text frames on a side mpsc the test reads.
- Closes with code 4000 on demand.

Hit `late-web`'s `/ws/spectate` over a tungstenite client, assert:
- The fake `/tunnel` saw `X-Late-View-Only: 1` and the configured
  spectator username/fingerprint.
- Binary `hello-world VT` arrives at the browser side intact.
- Browser-sent Resize Text frame is received upstream verbatim.
- Browser-sent Binary frame is **also** received upstream verbatim
  (proving late-web is a transparent proxy; backend-side enforcement
  is what makes this safe).
- Upstream close 4000 propagates to the browser-side socket.

testcontainers NOT required — the spectator path doesn't write to the DB
itself (the spectator user is created once by `ensure_user` on a real
late-ssh run, and POC1's tests don't exercise that path live).

**Manual / smoke (Phase E):**

- Real browser test for visual correctness — automated tests don't
  replace eyes on actual ratatui output.

**Test gaps deferred / called out:**

- Browser-side reconnect loop (post-POC1).
- Concurrency stress (N parallel spectators on one late-ssh pod).
- Mobile-browser quirks (touch event leakage past `disableStdin`,
  iOS Safari viewport sizing under FitAddon).
- True end-to-end test against a live late-ssh — relies on manual
  verification on l8.st.

## Risks / watch-outs

- **Activity-feed spam** — addressed by §Phase B step 5. Must ship
  before the page is publicly reachable on l8.st, otherwise every
  browser open/close emits "spectator joined / left" into every real
  SSH user's sidebar.
- **Conn-limit starvation** — spectators share
  `state.conn_limit` with real SSH users (`tunnel.rs:315`). §Phase B
  step 6 is the real fix; document an interim cap in PR notes.
- **`LATE_FORCE_ADMIN=1` in dev** — `session_bootstrap.rs:203` ORs that
  env var into `is_admin`. With `force_admin=1` the spectator becomes
  admin. Fine for compose dev. Confirm `l8.st` preview env has
  `force_admin=0` BEFORE exposing the page publicly. Already enforced
  in `infra/service-ssh.tf` for prod.
- **Real browser IP forwarding** — the `/tunnel` per-IP rate limiter
  keys on `X-Late-Peer-IP` (`tunnel.rs:302`). If late-web asserts its
  own pod IP there, all spectators collapse to one IP and rate-limiting
  trips immediately. Late-web must read `X-Forwarded-For` from
  NGINX/Caddy and forward as `X-Late-Peer-IP`. Pattern exists in
  `late-ssh/src/api.rs::effective_client_ip` — crib it.
- **Read-only enforcement is in late-ssh, not the proxy.** A direct
  attacker with the shared secret who reaches `/tunnel` (currently
  cluster-internal only) and omits `X-Late-View-Only: 1` would get a
  full-input session. The trust boundary stays at network reachability
  — same as today's bastion model. Don't add a backup gate at the
  proxy: it would make late-ssh a worse single source of truth and
  defeat the reason we chose backend-side enforcement (terminal-query
  response support, future per-session-state work).
- **xterm.js bundle size** — vendoring `@xterm/xterm` + `addon-fit`
  adds ~200KB compressed to `static/`. Acceptable for POC1; default
  ServeDir cache headers are fine.
- **Mobile / virtual keyboards** — even with `disableStdin: true`,
  mobile Safari can scroll/zoom the page. Out of scope for POC1; flag
  in CONTEXT.md after first user feedback.

## Phase summary

- **POC1 (this plan):** demo-mode read-only viewer, view-only enforced
  in late-ssh, single shared anonymous user, no audio, no reconnect,
  vendored xterm.js. Manual verification on l8.st only.
- **Future work (not in this plan):**
  - Per-session anonymous users with a harvest job (move off the shared
    `spectator` account).
  - Browser audio pairing reuse (POC2).
  - Browser → backend keystrokes (toggle `view_only` off per session).
  - Browser-side reconnect loop.
  - Production rollout to `late.sh` (NGINX ingress annotations,
    NetworkPolicy, CIDR allowlist — §Phase D step 11).
  - Spectator-vs-real-user conn-limit split (§Phase B step 6).
  - Mobile UX polish.
- **Companion artifact:** after POC1 ships, update `CONTEXT.md`
  §Architecture (add late-web as a `/tunnel` client) and §Future Work
  (track the deferred items above).
