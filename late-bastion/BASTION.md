# late-bastion

Long-lived SSH frontend for `late.sh`. Terminates user SSH connections, then tunnels the shell byte stream over a WebSocket to `late-ssh`'s `/tunnel` endpoint, transparently reconnecting across backend deploys.

> **Authoritative design doc.** [`devdocs/LATE-CONNECTION-BASTION.md`](../devdocs/LATE-CONNECTION-BASTION.md). The doc covers topology, protocol, security model, deployment, migration strategy, phasing, and decision log. This file is a quick orientation for in-repo readers.

## Why this crate exists

`late-ssh` ships often (TUI features, bug fixes, content). Its in-proc model means every deploy drops every active SSH session. The bastion is a thin, rarely-upgraded process that owns the user SSH endpoint and reconnects to `late-ssh` automatically on backend restart — keeping `ssh late.sh` alive across rollouts.

## Guiding principle

**Bastion is intentionally minimal — "no smarter than it needs to be to connect the wires."**

What it does:
- russh server (user-facing). Pubkey auth, `pty-req` + `shell` + `window-change` only.
- One outbound WebSocket per shell channel; pure byte-pump in both directions.
- Translates SSH `window-change` requests into WS `resize` text frames.
- Plain-text "reconnecting…" messages written directly to the SSH channel during backend gaps.
- One global connection-count semaphore. PROXY v1 parsing on the listener (NGINX → bastion).

What it does **not** do (deliberate):
- No DB, no service deps, no per-user logic, no per-IP limits, no ban awareness, no token issuance, no inspection of the TUI's terminal byte stream. All of that lives at `late-ssh`.

## Topology

```
ssh late.sh   ──▶  NGINX :22  TCP-passthrough  ──▶  service-ssh-sv:2222   (legacy, unchanged)
ssh -p 5222   ──▶  NGINX :5222 TCP-passthrough  ──▶  service-bastion-sv:5222
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

Both NGINX TCP entries run in parallel through Phase 4, gated on which port the user dials. Production cutover (Phase 5) is a one-line TF change in `infra/ssh-tcp.tf`.

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

- `LATE_BASTION_SSH_PORT` — listener port (`5222` during dual-path rollout).
- `LATE_BASTION_HOST_KEY_PATH` — file path for the bastion's russh host key.
- `LATE_BASTION_SSH_IDLE_TIMEOUT` — russh inactivity timeout, in seconds.
- `LATE_BASTION_BACKEND_TUNNEL_URL` — `ws://service-ssh-internal:4001/tunnel`.
- `LATE_BASTION_SHARED_SECRET` — pre-shared secret sent as `X-Late-Secret` on the WS upgrade.
- `LATE_BASTION_MAX_CONNS_GLOBAL` — global connection cap.
- `LATE_BASTION_PROXY_PROTOCOL` — `1` or `0`. Enable PROXY v1 parsing on the listener.
- `LATE_BASTION_PROXY_TRUSTED_CIDRS` — comma-separated CIDRs allowed to send PROXY v1 headers.

## Status

- ✅ **Phase 1** — crate scaffolded; russh server boots; host key load/generate; stub shell handler.
- ✅ **Phase 1** — PROXY v1 parser lifted to `late_core::proxy_protocol`.
- ✅ **Phase 2a** — `/tunnel` listener exists in `late-ssh` (`src/tunnel.rs`). Validates CIDR allowlist, pre-shared secret, and required handshake headers.
- ✅ **Phase 2 (protocol)** — `late_core::tunnel_protocol::ControlFrame::Resize { cols, rows }` wire schema for `window-change` forwarding. Header constants (`HEADER_*`) live alongside `ControlFrame` so both ends of the wire reference one source of truth.
- ✅ **Infra TF skeleton** — `service-ssh-internal-sv` ClusterIP for `:4001`, bastion Deployment/Service, NetworkPolicy, dual NGINX TCP entries (`:22` legacy, `:5222` bastion). Bastion side gated by `BASTION_ENABLED` (default `0`) until live cutover.
- ✅ **Phase 2b** — I/O seam refactor in `late-ssh` so `App::new(SessionConfig)` runs on either a russh `Channel` or a WS pair (`FrameSink` trait).
- ✅ **Phase 2c** — `/tunnel` constructs `SessionConfig` and runs the render loop over the WS streams; hand-written WS smoke client; full caller-side parity with `shell_request` (conn limits, active_users, activity feed, metrics).
- ✅ **Phase 3** — bastion proxy logic: dial `/tunnel`, pump bytes, forward `resize`, close SSH on WS close. Includes PROXY v1 parsing on the listener (CIDR-trusted) so `X-Late-Peer-IP` reflects the real client IP behind NGINX.
- ✅ **Phase 4** — reconnect loop + plain-text "reconnecting…" messages.
  - ✅ **4/1** — backend emits WS close 1000 on graceful drain (token-driven).
  - ✅ **4/2** — bastion reconnect loop: retryable closes (1000/1001/1006) and HTTP 5xx → exponential backoff (100ms→5s, 30s budget) with `X-Late-Reconnect: 1` and stable `X-Late-Session-Id`. Terminal closes (4000/4001/4002/4003) and HTTP 4xx end the session.
  - ✅ **4/3** — plain-text "reconnecting to late.sh…" written into the SSH stream after a 500ms gap (escalates to "still reconnecting…" at 5s). Preceded by a terminal-reset prefix (`\x1b[?1049l\x1b[0m\x1b[2J\x1b[H`) so the previous TUI's alt-screen / styling is cleared. Suppressed on the *first* dial — only fires when reopening a previously-good session.
  - ✅ **4/4** — bastion sends a WS Ping every 2s; backend's tungstenite layer auto-pongs. >5s of silence (no inbound frame of any kind) is treated as a wedged backend and breaks the pump into the reconnect loop. In-cluster RTT is sub-ms, so the threshold has plenty of slack.
  - ✅ **4/5** — live integration validated manually: bouncing the `late-ssh` container shows the reconnect message on the SSH client; restarting it replays the welcome sequence and lands the user back in the UI.
- ✅ **Ordering refactor** — both inbound paths (bastion handler, backend `/tunnel` receive loop) collapsed onto a single `mpsc<SshInputEvent>` (`Bytes` | `Resize`). Closes the prior `tokio::select!`-mux + eager-resize race that could surface `[A, R, B]` as `[R, AB]` to the app. Critical for coordinate-sensitive features (mouse SGR, paste, artboard). Validated manually against the artboard.
- ⏳ **Phase 5** — production cutover (`:22` swing).

## Running locally

The bastion runs as a docker-compose service alongside `late-ssh`. Both are
started by:

```bash
make start
```

This generates `.env` with dev defaults (see the `LATE_TUNNEL_*` and
`LATE_BASTION_*` blocks in the `Makefile`), brings up `service-ssh`,
`service-bastion`, postgres, and the audio stack. The bastion's russh host
key auto-generates on first boot at `/app/bastion_host_key` (gitignored,
persists in the repo dir via the `.:/app` bind mount).

Then in another shell:

```bash
# bastion path (new):
ssh -p 5222 -o StrictHostKeyChecking=no localhost
# legacy path (still up during dual-rollout):
ssh -p 2222 -o StrictHostKeyChecking=no localhost
```

Either path lands you in the same `late-ssh` UI; the bastion path proves the
end-to-end `/tunnel` plumbing. Bouncing the `late-ssh` container while
connected via `:5222` exercises the reconnect loop (Phase 4).

Override any var on the make line if needed, e.g.:

```bash
make start LATE_BASTION_SSH_PORT=2225 LATE_TUNNEL_SHARED_SECRET=hunter2
```

## Tests

Per the repo test policy ([`CONTEXT.md`](../CONTEXT.md) §Test Strategy):

- Pure-logic helpers (PROXY parsing, header construction, etc.) live as `#[cfg(test)] mod tests` blocks in their source file.
- Anything that needs a russh server, sockets, or a backend WS goes under `late-bastion/tests/` once we have meaningful integration surface (Phase 3+).
- LLM agents do not run `cargo test` / `cargo nextest` / `cargo clippy` here.
