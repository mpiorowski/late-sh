# late.sh SSH Bastion + WS Tunnel

> **Status.** Design through MVP scope is locked.
> **Branch.** `mevanlc--connection-bastion` (off `main`).
> **New crate.** `late-bastion` (the SSH-facing process). The previously-discussed name `late-gateway` is **not** used.

Long-lived SSH frontend that keeps the user's `ssh late.sh` session alive across `late-ssh` (TUI backend) deploys by terminating SSH at a stable bastion process and tunneling the shell byte stream over a WebSocket to the backend, transparently reconnecting when the backend restarts.

---

## 1. Goal & non-goals

### Goal
- User runs `ssh late.sh` once and stays connected across backend deploys.
- On backend restart, the screen visibly resets (no app-state continuity) — acceptable, an upgrade is the whole point of reconnecting.
- The bastion ships rarely; the backend ships as often as we like.

### MVP scope (v1)
- Identity persists across reconnect (SSH pubkey fingerprint is stable).
- All in-app TUI state is **expected to be lost** on reconnect. Users start fresh in the TUI after a backend redeploy.
- Single bastion replica is acceptable.

### Non-goals (v1)
- Cross-upgrade app-state continuity.
- Multi-region bastion / failover.
- Mosh-style client-side reconnect — vanilla `ssh` is the contract.
- Authenticating the user at the backend with their pubkey — the bastion owns SSH-layer auth.
- Routing `late-cli` through the bastion (see §11).
- Any per-user, per-IP, ban-aware, or otherwise stateful logic in the bastion. **Bastion is intentionally minimal — "no smarter than it needs to be to connect the wires."**

---

## 2. Process model context (why this is feasible)

`late-ssh` today is **a single process, single Tokio runtime, in-proc TUI**:

- One binary boots shared services once: DB pool, `VoteService`, `ChatService`, `ArticleService`, `ProfileService`, `NotificationService`, `SessionRegistry`, `dartboard_server`, etc.
- `ssh.rs` runs a russh server on `:2222`. Per connection, russh spawns an async task with a `ClientHandler`.
- On `pty-req` + `shell` request, that handler builds a `SessionConfig` (carrying `Arc` clones of the shared services), constructs `App::new(...)`, and enters the render loop — all inside the same Tokio task.
- The render loop synchronously calls `App::tick()` + `App::render()` and `write_all`s the rendered bytes into the russh `Channel`.

The seam we exploit: `App::new(SessionConfig)` and the render loop don't care whether the I/O sink is a russh `Channel` or a WebSocket stream. Swap the I/O, keep everything else.

---

## 3. Topology (parallel paths during rollout)

```
ssh late.sh                                    ssh -p 5222 late.sh
    │                                                │
    │   NGINX TCP passthrough (PROXY v1)             │   NGINX TCP passthrough (PROXY v1)
    ▼   :22  →  service-ssh-sv:2222                  ▼   :5222  →  service-bastion-sv:5222
    │                                                │
    ▼                                                ▼
late-ssh pod :2222 (russh)                  late-bastion pod :5222 (russh)
    │  in-proc                                       │
    │                                                │  ws://service-ssh-internal:4001/tunnel
    ▼                                                ▼
TUI render loop  ◀──── same App::new(SessionConfig) ────▶  late-ssh pod :4001 (axum /tunnel)
                                                                                │
                                                                                ▼ in-proc
                                                                            TUI render loop
```

Both NGINX entries are a single line each in `infra/ssh-tcp.tf`'s `HelmChartConfig`. Live in parallel during rollout; cut `:22` over to `service-bastion-sv:5222` when confident; rollback is one line in TF.

**Late-ssh exposes three listeners after this work:**

| Port  | Visibility              | Purpose                                                 |
| ----- | ----------------------- | ------------------------------------------------------- |
| 2222  | NGINX `:22` (today, legacy) | russh — direct SSH path; eventual ClusterIP-only.    |
| 4000  | `api.late.sh` ingress (today) | Public HTTP API — paired clients, etc. Unchanged.  |
| 4001  | ClusterIP-only (new)    | axum `/tunnel` WebSocket — bastion's only entry point.  |

`/tunnel` lives on a **separate listener / separate Service** from the public `:4000`. Mixing trust domains on one socket is a known footgun; keeping them on different binds gives kernel-level isolation in addition to in-app checks.

---

## 4. Protocol: bastion ↔ late-ssh

Transport: WebSocket over plain TCP inside the cluster (cheap and sufficient given NetworkPolicy + IP allowlist + shared secret). Upgrade to `wss://` if/when bastion and backend land on different hosts.

### Handshake (HTTP upgrade)

Bastion opens `GET /tunnel` with headers:

| Header                | Purpose                                                                 |
| --------------------- | ----------------------------------------------------------------------- |
| `X-Late-Secret`       | Pre-shared secret. Constant-time compare. Rotated out-of-band.         |
| `X-Late-Fingerprint`  | User's SSH pubkey fingerprint (authenticated by bastion).               |
| `X-Late-Username`     | Optional hint; backend re-derives via DB lookup (fingerprint is authoritative). |
| `X-Late-Peer-IP`      | Real client IP, captured by bastion from PROXY v1 header. Used by late-ssh's per-IP rate limiter. |
| `X-Late-Term`         | `$TERM` from `pty-req`.                                                 |
| `X-Late-Cols`         | Columns at handshake time.                                              |
| `X-Late-Rows`         | Rows at handshake time.                                                 |
| `X-Late-Reconnect`    | `1` if this is a post-upgrade reconnect for the same user-SSH session. **Logged for correlation only — late-ssh does not change behavior based on it.** |
| `X-Late-Session-Id`   | Bastion-minted UUIDv7, stable across reconnects (logs/metrics correlation). |

Backend validates secret + IP allowlist, looks up user by fingerprint, allocates the TUI session.

### Frames (post-handshake)

- **Binary frames**: raw PTY bytes, both directions. **No inspection, no escape-sequence parsing.** late-ssh emits OSC/CSI-rich output (artboard, cursor styling, alt-screen toggling, etc.); the bastion treats every binary frame as opaque.
- **Text frames** (JSON control vocabulary, intentionally tiny):
  - `{"t":"resize","cols":N,"rows":M}` — bastion → backend on SSH `window-change` request from the user's client.
  - `{"t":"hello"}` — backend → bastion (optional), confirms session ready.
  - Anything else is out of scope for v1.
- **Ping/pong**: WS control frames. Bastion pings every 2s; >5s of silence treated as a dead backend.

### Close codes

| Code   | Meaning                  | Bastion action                          |
| ------ | ------------------------ | --------------------------------------- |
| `1000` | Normal (graceful drain)  | Reconnect silently if fast; show reconnect message after 500ms. |
| `1001` | Going away               | Same.                                   |
| `1006` | Abnormal / transport     | Reconnect with backoff.                 |
| `4000` | Session ended by backend (user quit, render error) | Close user's SSH session cleanly. |
| `4001` | Kicked by backend        | Close user's SSH session cleanly.       |
| `4002` | Auth revoked / banned / unknown user | Close user's SSH session cleanly. |
| `4003` | Protocol error           | Close user's SSH session cleanly.       |

---

## 5. Bastion responsibilities (`late-bastion` crate)

Guiding principle: **the bastion is intentionally minimal.** No DB, no service dependencies, no protocol-aware logic over the byte stream. Goal is high uptime and near-zero need to redeploy.

What it does:

- Full SSH server (russh): pubkey auth, channel management, `pty-req` + `shell` + `window-change` only. Reject everything else (`exec`, `subsystem`, port forwarding, additional shell channels) cleanly.
- Host key via the existing `load_or_generate_key` pattern (mirrored from `late-ssh`), separate `bastion_key` path mounted from a K8s Secret.
- PROXY v1 parsing on its `:5222` listener. Parser is shared with late-ssh via a small `late-core` util module (lifted in this work).
- One outbound WebSocket per open shell channel.
- **Pure byte pump in both directions; no inspection of the shell byte stream.** Key invariant.
- Translate SSH `window-change` requests into `resize` text frames.
- A single global connection-count semaphore to bound resource usage. **No per-IP enforcement — that stays at late-ssh, keyed on `X-Late-Peer-IP`.**
- On WS close with retryable code:
  1. If reconnect is fast (<500ms), say nothing — just resume.
  2. Otherwise, write a small plain-text "reconnecting to late.sh…" message directly to the SSH channel. Reset terminal state first (`\x1b[?1049l\x1b[0m\x1b[2J\x1b[H`) to exit any active alt-screen and clear formatting. (This is text written by the bastion, not a TUI render — it's the only point at which the bastion writes its own bytes to the user's screen.)
  3. Reconnect loop with exponential backoff (100ms → 5s cap), max ~30s total. After 5s, message escalates to "still reconnecting…".
  4. On success, the new TUI's setup sequences cleanly overwrite the message; resume piping bytes.
  5. On timeout, close the SSH session with a friendly final message.
- On SIGTERM: write a brief goodbye to all active SSH channels, close cleanly, exit. Single-replica means active SSH-over-bastion sessions are dropped during bastion rollouts (acceptable per §1).
- No persistent state beyond per-session "this SSH channel maps to this backend URL + session id + current PTY size."

What it does **not** do (intentional non-responsibilities):
- No DB lookup, no user-existence check, no ban check. Late-ssh handles all of that and uses close code `4002` to reject.
- No per-IP rate limiting. Late-ssh keeps existing per-IP caps using `X-Late-Peer-IP`.
- No interpretation of, or coupling to, the TUI's terminal output.
- No `X-Late-Reconnect`-derived behavior other than setting the header.

---

## 6. Backend (`late-ssh`) changes

Additive — nothing existing is removed in MVP.

- Add a second axum listener on a private port (`:4001`, ClusterIP-only Service).
- `/tunnel` handler:
  - Validates `X-Late-Secret` (constant-time) and peer IP against an in-app allowlist (env-config).
  - Looks up / creates user from `X-Late-Fingerprint`. Banned or otherwise-rejected users get close code `4002`.
  - Constructs `SessionConfig` and an I/O-stream-shaped abstraction over the WS, then runs `App::new(...)` exactly as the russh path does.
  - Handles `resize` text frames by forwarding to the same pty-resize-equivalent path used today by the russh `window_change_request` callback.
  - Emits close code `1000` on graceful shutdown (SIGTERM drain), `4001`/`4002` on explicit reject.
- **Per-IP rate limiter**: existing limiter is reused; for `/tunnel` sessions the IP key comes from `X-Late-Peer-IP`. Existing russh path on `:2222` continues to use the connection's transport peer addr (unchanged).
- **`X-Late-Reconnect`**: logged for correlation only. The backend treats every `/tunnel` session identically — no special-cased splash skip, no welcome-overlay bypass, no behavioral branch. KISS; revisit later if reconnect-UX feels janky.
- Refactor: extract the "build SessionConfig + run TUI on this I/O pair" path so russh and `/tunnel` share it. This is the core code change.
- Existing russh on `:2222` is **unchanged** for MVP. `late-cli` keeps using it directly.
- Session state continues to live only in the backend process and DB — no cross-upgrade in-memory state needed.

---

## 7. Security model

The bastion is the trust root for user pubkey auth. The backend never sees user key material — only a fingerprint asserted by the bastion. Layered defense ensures only a real `late-bastion` pod can reach `/tunnel`.

### Layer 1 — Network isolation (Service + NetworkPolicy)
- `:4001` Service is `ClusterIP` only (no Ingress, no NodePort, no NGINX exposure).
- `NetworkPolicy` allows ingress to `:4001` only from the `late-bastion` pod.
- Public ingress to `:4001` is impossible: there is no path.

### Layer 2 — In-app IP allowlist
- The `/tunnel` handler checks the peer IP against a configured allowlist before accepting the upgrade; reject with HTTP 403 otherwise.
- Allowlist lives in backend config (env / config file), not in the NetworkPolicy alone. A future VPC change or NetworkPolicy typo must not silently expose `/tunnel` — the app refuses regardless.
- Deliberately duplicative of Layer 1.

### Layer 3 — Pre-shared secret
- `X-Late-Secret` header on the WS upgrade. Constant-time compare.
- Rotate by deploying new secret to backend first (accept old + new for a brief overlap), then to the bastion.
- Scoped to "this is a late-sh bastion" — not per-user, not per-session.

### Layer 4 — Transport
- Plain WS inside the cluster is fine for MVP (Layers 1–3 suffice).
- `wss://` once bastion and backend are on different hosts / cross-network.

### Future (not v1)
- Signed bastion identity (short-lived JWT signed by an operator key, `Authorization` header). Lets us revoke a single bastion without rotating a shared secret. Revisit when we have >1 bastion or a key-rotation pain point.

### What the backend does NOT trust
- The user's SSH pubkey (never sees it).
- Any auth claim on the WS that isn't backed by Layers 1–3.
- `X-Late-Username` — hint only; fingerprint is authoritative.

### Properties worth restating
- **The bastion does not inspect terminal bytes.** Every byte the TUI emits is opaque to it. Keeps the bastion thin (no terminal-protocol awareness, no version-coupling to the TUI) and is the primary reason WS won out over telnet (telnet's IAC byte-stuffing requires inspection) or raw-TCP-with-our-own-framing (we'd reinvent WS, badly).
- **The bastion has no DB, no service deps, no per-user logic.** This is a uptime-maximization choice: minimum surface for bugs that could force a bastion restart and drop every active SSH session.

---

## 8. Deployment

### Infra delta (`infra/`)

- **`infra/service-bastion.tf` (new)** — Deployment + Service for `late-bastion`. Mirrors `service-ssh.tf` shape. Container port 5222, exposed via Service. Resources modest (no DB, no shared services — just russh + WS).
- **`infra/ssh-tcp.tf` (modify)** — Add a second TCP entry to the NGINX `HelmChartConfig`:
  ```yaml
  tcp:
    "22":   "default/service-ssh-sv:2222::PROXY"      # legacy, unchanged through MVP
    "5222": "default/service-bastion-sv:5222::PROXY"  # new bastion path
  ```
- **`infra/service-ssh.tf` (modify)** — Add a second container port `:4001` and a new `service-ssh-internal-sv` ClusterIP Service exposing only `:4001`. Public Service for `:4000` unchanged.
- **NetworkPolicy (new)** — In a new `infra/network-policies.tf` (or appended): allow ingress to `service-ssh-sv:4001` only from pods labeled `app=service-bastion`.
- **Secrets (`infra/secrets.tf`)** — New `BASTION_SHARED_SECRET` mounted into both `late-bastion` and `late-ssh`. New `BASTION_HOST_KEY` Secret (or similar) for the bastion's russh host key.

### Rolling backend deploy (post-cutover)

1. New `late-ssh` pod comes up; `/tunnel` healthy.
2. Old `late-ssh` pod gets SIGTERM; on shutdown it sends WS close `1000` to all tunnel sessions, drains, exits.
3. Bastion reconnects each session to the new pod — most reconnects complete inside 500ms (no message shown); slower ones see a brief plain-text "reconnecting…" message that gets cleanly overwritten when the new TUI initializes.

### Bastion upgrades

Rare. When needed, accept that all SSH-over-bastion users get dropped (plan for low-traffic hours). Multi-replica bastion is a future enhancement.

---

## 9. Migration strategy

Two paths run **in parallel**, in production, controlled by which port the user dials:

| Phase | `:22`                          | `:5222`                        | Audience                            |
| ----- | ------------------------------ | ------------------------------ | ----------------------------------- |
| 0     | → `service-ssh-sv:2222` (today) | (does not exist)              | All users.                          |
| 1     | → `service-ssh-sv:2222` (today) | → `service-bastion-sv:5222`   | Default users on `:22`. Dogfooders on `:5222`. |
| 2     | → `service-bastion-sv:5222`    | → `service-bastion-sv:5222`   | All users go through bastion. `:5222` kept as escape valve. |
| 3     | → `service-bastion-sv:5222`    | (removed)                      | Steady state.                       |

`:22 → bastion` cutover is a **one-line TF change**. Rollback is the same one-line TF change in reverse.

`late-cli` continues to dial `late.sh:22` (or any new dedicated late-cli port) directly to `service-ssh-sv:2222` until we choose to migrate it. Decoupled from this work — see §11.

---

## 10. Phased implementation

Each phase is independently mergeable and behind opt-in (the `:5222` dogfood port).

### Phase 0 — Decisions (done)
Captured throughout this doc.

### Phase 1 — Crate scaffold
- Cut `mevanlc--connection-bastion` off `main`.
- Add `late-bastion` to workspace; minimal russh server skeleton (key load/gen, accept on `:5222`, log channel events). No proxy logic yet.
- Lift PROXY v1 parser to `late-core`. Late-ssh re-points to the shared util. Late-bastion uses it on day 1.
- Drop a `BASTION.md` in the crate dir summarizing this doc for in-repo readers.

### Phase 2 — Backend `/tunnel` endpoint
- Add `:4001` listener + `/tunnel` axum WS handler in `late-ssh`.
- Refactor session bootstrap so russh and `/tunnel` share the "build `SessionConfig`, run TUI on this I/O" path.
- Wire per-IP rate limiter to `X-Late-Peer-IP` for `/tunnel` sessions.
- Smoke test against a hand-written WS client (no bastion yet). Existing russh path unchanged.

### Phase 3 — Bastion proxy logic
- Bastion: on `pty-req` + `shell`, dial `/tunnel`, pump bytes, forward `resize`, close SSH on WS close (any code).
- Wire NGINX `:5222` → bastion. Confirm full TUI works end-to-end via bastion at `ssh -p 5222 late.sh`.

### Phase 4 — Reconnect loop
- On close `1000`/`1001`/`1006`, write the plain-text reconnect message (after grace period), reconnect with backoff.
- Verify `Channel::data()` correctly pushes bastion-authored bytes to the user's still-open SSH channel mid-life. (See §11.)
- Verify against a manual backend restart while a session is active.

### Phase 5 — Production cutover
- Keep both paths up for a soak window.
- Decide and ship `late-cli`'s post-cutover routing (see §11).
- Flip `:22` TF entry from `service-ssh-sv:2222` to `service-bastion-sv:5222`.
- Observe; rollback in-place if needed.

### Phase 6 — Nice-to-haves (not v1)
- Multi-replica bastion.
- Metrics: reconnect count, reconnect duration, close-code distribution, handshake latency, bytes pumped.
- Move per-IP rate limiting / abuse defense into the bastion if profiling justifies it (only if the perf or abuse-pattern data demands).
- `late-cli` migration through the bastion (or a parallel WS endpoint).

---

## 11. Open questions

These are the only items genuinely deferred:

- **`late-cli` routing after `:22` cutover (Phase 5 decision).** Default plan: expose late-ssh's russh on a separate public port (e.g. `:2223`) just for late-cli; late-cli changes default port. Alternatives (route through bastion via parallel WS exec endpoint, or have bastion mint tokens itself) remain on the table. Decision needs to land before Phase 5 ships.
- **Verify `Channel::data()` mid-channel-life behavior (Phase 4 implementation check).** Expected: works as a plain byte write; no special "between sessions" mode in russh. Confirm by trying it. Surface as a real issue if it doesn't.
- **Reconnect message wording (Phase 4 polish).** Strings like "reconnecting to late.sh…" / "still reconnecting…" are tunable; iterate during Phase 4.

---

## 12. What we explicitly considered and rejected

- **Telnet protocol** between bastion and backend. Telnet's IAC byte (`0xFF`) requires byte-stuffing/unstuffing in the data stream — that *is* terminal-byte inspection, contradicting the bastion's "thin / opaque pump" property.
- **Raw TCP with bespoke framing.** Reinvents WebSocket, badly. WS already solves "single connection carrying both an opaque byte stream and a small control vocabulary, with clean shutdown semantics."
- **Mixing `/tunnel` onto the existing public `:4000`** API listener. Saves a Service but removes kernel-level isolation between the public API surface and the trust-the-headers tunnel route. Footgun.
- **Big-bang `:22` cutover.** Replaced by parallel-path rollout via dual NGINX TCP entries.
- **Full SSH-in-SSH proxy** (bastion as russh server + russh client). Works, but doubles SSH framing/crypto on the inner hop and forces an exec-request gymnastics for identity passing. WS is simpler at every layer.
- **Bastion holding any per-user, per-IP, or ban-aware state.** Explicitly rejected to maximize bastion uptime. Late-ssh owns all such logic; bastion is "no smarter than it needs to be to connect the wires."
- **Backend skipping splash/welcome on `X-Late-Reconnect=1`.** Considered; rejected for MVP on KISS grounds. Header is preserved in the protocol for log correlation (and as future-optional behavior) but does not branch backend code today.
