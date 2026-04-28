# late-bastion

Long-lived SSH frontend for `late.sh`. Terminates user SSH connections, then tunnels the shell byte stream over a WebSocket to `late-ssh`'s `/tunnel` endpoint, transparently reconnecting across backend deploys.

> **Authoritative design doc.** [`PERSISTENT-CONNECTION-GATEWAY.md`](../../../aihome/late-sh/PERSISTENT-CONNECTION-GATEWAY.md) (lives outside the repo). The doc covers topology, protocol, security model, deployment, migration strategy, phasing, and decision log. This file is a quick orientation for in-repo readers.

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
| 1     | Crate scaffold (you are here): russh skeleton, host key, connection accept, stub shell reply. | All MVP surface stubbed.  |
| 2     | Backend `/tunnel` endpoint in `late-ssh`. Hand-written WS client smoke test.                   | None.                     |
| 3     | Bastion proxy logic: dial `/tunnel`, byte-pump, forward `window-change` as `resize` frames.    | Most of the real work.    |
| 4     | Reconnect loop + plain-text reconnect messages.                                                | Polish.                   |
| 5     | Production cutover (`:22` swing).                                                              | None (infra only).        |

Phase numbers track [`PERSISTENT-CONNECTION-GATEWAY.md`](../../../aihome/late-sh/PERSISTENT-CONNECTION-GATEWAY.md) §10.

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

## Status (Phase 1)

- ✅ Crate scaffolded; russh server boots on the configured port.
- ✅ Host key load/generate path mirrors `late-ssh`.
- ✅ Stub shell handler — connecting to the bastion writes a placeholder banner and closes cleanly.
- ⏳ PROXY v1 parsing not wired yet (lift to `late-core` is part of Phase 1; following commit).
- ⏳ Backend `/tunnel` endpoint not yet built (Phase 2).
- ⏳ No proxy logic; no WS client; no reconnect (Phase 3 / Phase 4).

## Running locally (Phase 1, smoke only)

```bash
LATE_BASTION_SSH_PORT=5222 \
LATE_BASTION_HOST_KEY_PATH=/tmp/bastion_host_key \
LATE_BASTION_SSH_IDLE_TIMEOUT=300 \
LATE_BASTION_BACKEND_TUNNEL_URL=ws://localhost:4001/tunnel \
LATE_BASTION_SHARED_SECRET=dev-only-not-a-real-secret \
LATE_BASTION_MAX_CONNS_GLOBAL=1024 \
LATE_BASTION_PROXY_PROTOCOL=0 \
LATE_BASTION_PROXY_TRUSTED_CIDRS= \
cargo run -p late-bastion
```

Then in another shell:

```bash
ssh -p 5222 -o StrictHostKeyChecking=no localhost
```

Expected: SSH connects, you see `late-bastion stub: tunnel not yet wired (Phase 3).`, the channel closes cleanly. The `~/.ssh/id_*` you connect with becomes the asserted fingerprint for the (eventual) WS handshake.

## Tests

Per the repo test policy ([`CONTEXT.md`](../CONTEXT.md) §Test Strategy):

- Pure-logic helpers (PROXY parsing, header construction, etc.) live as `#[cfg(test)] mod tests` blocks in their source file.
- Anything that needs a russh server, sockets, or a backend WS goes under `late-bastion/tests/` once we have meaningful integration surface (Phase 3+).
- LLM agents do not run `cargo test` / `cargo nextest` / `cargo clippy` here.
