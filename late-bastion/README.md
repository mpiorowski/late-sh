# late-bastion

Long-lived SSH frontend for `late.sh`. Terminates user SSH connections, then tunnels the shell byte stream over a WebSocket to `late-ssh`'s `/tunnel` endpoint, transparently reconnecting across backend deploys.

> **Authoritative design doc.** [`devdocs/LATE-CONNECTION-BASTION.md`](../devdocs/LATE-CONNECTION-BASTION.md). The doc covers topology, protocol, security model, deployment, migration strategy, phasing, and decision log. This file is a quick orientation for in-repo readers.

## Why this crate exists

`late-ssh` ships often (TUI features, bug fixes, content). Its in-proc model means every deploy drops every active SSH session. The bastion is a thin, rarely-upgraded process that owns the user SSH endpoint and reconnects to `late-ssh` automatically on backend restart — keeping `ssh late.sh` alive across rollouts.

## Guiding principle

**Bastion is intentionally minimal — "no smarter than it needs to be to connect the wires."**

What it does:
- russh server (user-facing). Pubkey auth, PTY, shell, resize, whitelisted env,
  and a bounded pre-shell exec for CLI token discovery.
- One outbound WebSocket per shell channel; pure byte-pump in both directions.
- Translates SSH `window-change` requests into WS `resize` text frames.
- Plain-text "reconnecting…" messages written directly to the SSH channel during backend gaps.
- Global and per-IP connection caps plus a per-IP attempt limiter before SSH
  negotiation. PROXY v1 parsing on the listener supplies the trusted client IP.

What it does **not** do (deliberate):
- No DB, no service deps, no per-user logic, no ban awareness, no token issuance,
  no inspection of the TUI's terminal byte stream. Account admission stays at
  `late-ssh`; bastion's limits protect its own pre-auth transport resources.

## Topology

```
ssh late.sh   ──▶  ssh-edge :22 (PROXY v1) ──▶ service-bastion-sv:5222
                                                  │
                                                  ▼
                                           late-bastion (this crate)
                                                  │
                                                  ▼  ws://service-ssh-internal:4001/tunnel
                                           late-ssh /tunnel handler
                                                  │
                                                  ▼  in-proc
                                           App::new(SessionConfig)
```

This is the topology after the one-time maintenance cutover. Before it,
`BASTION_ENABLED=0` routes public `:22` from `ssh-edge` to direct
`service-ssh-sv:2222`. Setting it to `1` routes the same public port to the
bastion; there is no public dogfood port. See the
[cutover runbook](../infra/RUNBOOK.md#first-bastion-and-ipv6-http-cutover).

## Implementation phases

| Phase | Scope                                                                                          | This crate's surface area |
| ----- | ---------------------------------------------------------------------------------------------- | ------------------------- |
| 1     | Crate scaffold: russh skeleton, host key, connection accept, stub shell reply.                 | All MVP surface stubbed.  |
| 2     | Backend `/tunnel` endpoint in `late-ssh`. Hand-written WS client smoke test.                   | None.                     |
| 3     | Bastion proxy logic: dial `/tunnel`, byte-pump, forward `window-change` as `resize` frames.    | Most of the real work.    |
| 4     | Reconnect loop + plain-text reconnect messages.                                                | Polish.                   |
| 5     | Production cutover (`:22` swing).                                                              | None (infra only).        |

Phase numbers track [`devdocs/LATE-CONNECTION-BASTION.md`](../devdocs/LATE-CONNECTION-BASTION.md) §10.

## Configuration

Env-driven. See `src/config.rs` for the canonical list. Required vars:

- `LATE_BASTION_SSH_PORT` — internal listener port (`5222` by default; public SSH remains `:22`).
- `LATE_BASTION_HOST_KEY_PATH` — file path for the bastion's russh host key.
- `LATE_BASTION_SSH_IDLE_TIMEOUT` — russh inactivity timeout, in seconds.
- `LATE_BASTION_BACKEND_TUNNEL_URL` — `ws://service-ssh-internal:4001/tunnel`.
- `LATE_BASTION_SHARED_SECRET` — pre-shared secret sent as `X-Late-Secret` on the WS upgrade.
- `LATE_BASTION_MAX_CONNS_GLOBAL` — global connection cap.
- `LATE_BASTION_PROXY_PROTOCOL` — `1` or `0`. Enable PROXY v1 parsing on the listener.
- `LATE_BASTION_PROXY_TRUSTED_CIDRS` — comma-separated CIDRs allowed to send PROXY v1 headers.

Optional admission settings default to the direct-SSH limits:

- `LATE_BASTION_MAX_CONNS_PER_IP` — simultaneous connections per client IP (3).
- `LATE_BASTION_MAX_ATTEMPTS_PER_IP` — connection attempts per window (30).
- `LATE_BASTION_RATE_LIMIT_WINDOW_SECS` — attempt window (60 seconds).

The global cap also bounds pending PROXY reads. Per-IP checks run after trusted
PROXY resolution and before SSH negotiation. Disconnect, auth timeout and
cancellation release the connection slot; reconnecting does not reset attempts.
Trusted PROXY connections must supply a client address: missing, malformed or
`PROXY UNKNOWN` headers are rejected before SSH negotiation. The proxy's own
address is never substituted for an unknown client IP.

## Status

- ✅ **Phase 1** — crate scaffolded; russh server boots; host key load/generate; stub shell handler.
- ✅ **Phase 1** — PROXY v1 parser lifted to `late_core::proxy_protocol`.
- ✅ **Phase 2a** — `/tunnel` listener exists in `late-ssh` (`src/tunnel.rs`). Validates CIDR allowlist, pre-shared secret, and required handshake headers.
- ✅ **Phase 2 (protocol)** — `late_core::tunnel_protocol::ControlFrame::Resize { cols, rows }` wire schema for `window-change` forwarding. Header constants (`HEADER_*`) live alongside `ControlFrame` so both ends of the wire reference one source of truth.
- ✅ **Infra TF skeleton** — `service-ssh-internal-sv` ClusterIP for `:4001`, bastion Deployment/Service and NetworkPolicy. `ssh-edge` switches public `:22` to the bastion when `BASTION_ENABLED=1` (default `0` until the maintenance cutover).
- ✅ **Phase 2b** — I/O seam refactor in `late-ssh` so `App::new(SessionConfig)` runs on either a russh `Channel` or a WS pair (`FrameSink` trait).
- ✅ **Phase 2c** — `/tunnel` constructs `SessionConfig` and runs the render loop over the WS streams; hand-written WS smoke client; full caller-side parity with `shell_request` (conn limits, active_users, activity feed, metrics).
- ✅ **Phase 3** — bastion proxy logic: dial `/tunnel`, pump bytes, forward `resize`, close SSH on WS close. Includes PROXY v1 parsing on the listener (CIDR-trusted) so `X-Late-Peer-IP` reflects the real client IP behind `ssh-edge`.
- ✅ **Phase 4** — reconnect loop + plain-text "reconnecting…" messages.
  - ✅ **4/1** — existing tunnel sessions ride out graceful drain; the backend does not emit a healthy-session close on SIGTERM.
  - ✅ **4/2** — bastion reconnect loop: abnormal transport drops (`1006`) and late-private retryable closes (`4100`-`4199`) plus HTTP 5xx → exponential backoff (100ms→5s ceiling, each delay jittered to 50–100% of its ceiling, 30s budget) with `X-Late-Reconnect-Reason: <close-code>` and stable `X-Late-Session-Id`. Terminal closes (4000/4001/4002/4003) and HTTP 4xx end the session.
  - ✅ **4/3** — plain-text "connecting to late.sh…" or "reconnecting to late.sh…" after a 500ms gap (escalates at 5s), preceded by a terminal reset. Startup waits also show progress. User-requested `4100` redials show "waiting for updated late.sh..." immediately. Terminal rejections and exhausted outage budgets explain the failure before closing; backend HTTP 401/403 responses are logged at WARN.
  - ✅ **4/4** — bastion sends a WS Ping every 2s; backend's tungstenite layer auto-pongs. After first output, >5s of silence is treated as a wedged backend and starts reconnect. Bootstrap uses the 30s ready deadline instead; a successful client write restarts the silence interval so SSH backpressure is not mistaken for backend failure. Both output legs allow a blocked write up to 30s.
  - ✅ **4/5** — live integration validated manually: bouncing the `late-ssh` container shows the reconnect message on the SSH client; restarting it replays the welcome sequence and lands the user back in the UI.
- ✅ **Ordering refactor** — both inbound paths (bastion handler, backend `/tunnel` receive loop) collapsed onto a single `mpsc<SshInputEvent>` (`Bytes` | `Resize`). Closes the prior `tokio::select!`-mux + eager-resize race that could surface `[A, R, B]` as `[R, AB]` to the app. Critical for coordinate-sensitive features (mouse SGR, paste, artboard). Validated manually against the artboard.
- ⏳ **Phase 5** — production maintenance cutover (`:22` swing and IPv6 HTTP ingress handoff).

## Running locally

The bastion runs as a docker-compose service alongside `late-ssh`. Both are
started by:

```bash
make start
```

This generates `.env` from `.env.dev`, brings up `service-ssh`,
`service-bastion`, postgres, and the audio stack. The bastion's russh host
key auto-generates on first boot at `/app/bastion_host_key` (gitignored,
persists in the repo dir via the `.:/app` bind mount).

Then in another shell:

```bash
# bastion path:
ssh -p 5222 -o StrictHostKeyChecking=no localhost
# direct backend path for local comparison:
ssh -p 2222 -o StrictHostKeyChecking=no localhost
```

Either path lands you in the same `late-ssh` UI; the bastion path proves the
end-to-end `/tunnel` plumbing. Bouncing the `late-ssh` container while
connected via `:5222` exercises the reconnect loop (Phase 4).

## Tests

```bash
make test-bastion CHECK_BUILD_JOBS=4 # real listener + DB-backed backend regressions
make test-bastion-repeat            # five lifecycle regressions, 100 iterations each
make test-bastion-stack             # packaged release images, 10 clients, two replacements
make test-bastion-soak              # 45 minutes, 100 clients, ten replacements
```

The fixture uses disposable credentials, host keys, PostgreSQL and blue/green
backends behind HAProxy; it never loads `.env` or `.env.local`. All published
ports bind loopback and backend networks are internal. Reports survive under
`target/bastion-qualification/`; containers and volumes are removed on exit.
Socket regressions live beside their implementations in `proxy_test.rs` and
`late-ssh/src/bastion_test.rs`, using the production accept loop.

Reconnects preserve identity, latest terminal dimensions and whitelisted env.
Logs correlate sessions by a hash; the stable wire session ID is a pairing
bearer token and is never logged in full.
Repeated env names keep their latest value; each value is limited to 4 KiB.
The app restarts: queued outage keystrokes and paste are discarded, including
input before the replacement emits its first binary output. Backoff, stalled
upgrades and repeated empty upgrades share one 30-second outage budget. Input
and output waits are bounded, and a slow reader cannot prevent shutdown.
Normal app quits send SSH exit status 0 before EOF/Close; kick and ban closes
send status 1, including when the client uses OpenSSH ControlMaster.
Bastion restores mouse, paste, color and cursor modes before reconnect notices
and on final channel teardown, including when no backend cleanup arrived. Final
cleanup preserves the main screen and any farewell text.

See [BASTION-QUALIFICATION.md](../devdocs/BASTION-QUALIFICATION.md) for the
campaign, observed results and remaining staging checks. Automated agents follow
the repository test policy unless the user explicitly authorizes a broader run.
