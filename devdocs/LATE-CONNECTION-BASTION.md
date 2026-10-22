# late.sh SSH Bastion + WS Tunnel

> **Status.** Bastion path implemented; production cutover awaits staging checks. Remaining review work is tracked in [§13](#13-remaining-review-follow-ups).
> **Branch.** `mevanlc--bastion-redux`.
> **New crate.** `late-bastion` (the SSH-facing process).

Long-lived SSH frontend that keeps the user's `ssh late.sh` session alive across `late-ssh` (TUI backend) deploys by terminating SSH at a stable bastion process and tunneling the shell byte stream over a WebSocket to the backend, with explicit user agency over reconnect during upgrades.

The backend tunnel listener is compiled into the `late-ssh` profile: port `4002` in `dev`/`dev2` (the API uses `4001`), and port `4001` in `prod` (the API uses `4000`). Production trusts bastion peers in `10.42.0.0/16`; dev profiles accept local Compose peers. Dev profiles require `LATE_TUNNEL_SHARED_SECRET`; an absent or blank production secret leaves the private listener off while direct SSH continues to start. Compose maps the tunnel to host port `4002` for instance 1 and `4003` for instance 2, and maps bastion SSH to `5222`/`5223`. The bastion's configuration stays in `LATE_BASTION_*` environment variables.

Release tags ending in `-bastion` use `deploy_service.yml` to roll the bastion image. Terraform's `IMAGE_TAGS["bastion"]` seeds its initial image; subsequent infrastructure applies preserve the deployed image.

First enablement follows the [infra bastion instructions](../infra/README.md#bastion): deploy the ssh image, configure the host-key and shared-secret GitHub environment secrets, enable bastion through an `-infra` release, then publish a `-bastion` release. The infra apply creates the manifest without waiting for the placeholder image; the bastion release installs the real image and waits for readiness.

---

## 1. Goal & non-goals

### Goal
- User runs `ssh late.sh` once and stays connected across backend deploys.
- During an upgrade, the user gets **explicit agency**: choose to reconnect to the new version, disconnect cleanly, or stay on the old version until SIGKILL.
- Users who weren't actively engaged when SIGKILL hit get **silently reconnected with a contextual explanation**, not a silent teleport into fresh state.
- The bastion ships rarely; the backend ships as often as we like.

### MVP scope (v1)
- Identity persists across reconnect (SSH pubkey fingerprint is stable).
- All in-app TUI state is **expected to be lost** on reconnect.
- Single bastion replica is acceptable.

### Non-goals (v1)
- Cross-upgrade app-state continuity.
- Multi-region bastion / failover.
- Mosh-style client-side reconnect — vanilla `ssh` is the contract.
- Authenticating the user at the backend with their pubkey — bastion owns SSH-layer auth.
- Routing `late-cli` through the bastion (see §11).
- Per-user / ban-aware logic in the bastion. Account decisions stay at late-ssh;
  transport-level per-IP limits protect bastion's own SSH listener.
- Tailored "what's new in this release" messaging. Deferred — depends on Cargo workspace version bumping in CI (see §11).

---

## 2. Process model context

`late-ssh` today is a single process, single Tokio runtime, in-proc TUI. The seam we exploit: `App::new(SessionConfig)` and the render loop don't care whether the I/O sink is a russh `Channel` or a WebSocket stream. Swap the I/O, keep everything else.

---

## 3. Topology (parallel paths during rollout)

```
ssh late.sh                                    ssh -p 5222 late.sh
    │                                                │
    │   HAProxy TCP passthrough (PROXY v1)           │   HAProxy TCP passthrough (PROXY v1)
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

Both listeners live in the `ssh-edge` HAProxy configuration in `infra/ssh-tcp.tf`, on IPv4 and IPv6. `BASTION_ENABLED=1` enables the dogfood listener. Cut `:22` over by changing its `default_backend` from `service_ssh` to `service_bastion` when confident; rollback restores that value.

**Late-ssh exposes three listeners after this work:**

| Port  | Visibility              | Purpose                                                 |
| ----- | ----------------------- | ------------------------------------------------------- |
| 2222  | HAProxy `:22` (legacy)  | russh — direct SSH path; eventual ClusterIP-only.       |
| 4000  | `api.late.sh` ingress   | Public HTTP API — paired clients, etc. Unchanged.       |
| 4001  | ClusterIP-only (new)    | axum `/tunnel` WebSocket — bastion's only entry point.  |

`/tunnel` is a separate listener / separate Service from public `:4000`. Mixing trust domains on one socket is a known footgun.

---

## 4. Protocol: bastion ↔ late-ssh

Transport: WebSocket over plain TCP inside the cluster (NetworkPolicy + IP allowlist + shared secret are sufficient). Upgrade to `wss://` if/when bastion and backend land on different hosts.

### Handshake (HTTP upgrade)

Bastion opens `GET /tunnel` with these headers:

| Header                    | Purpose                                                                 |
| ------------------------- | ----------------------------------------------------------------------- |
| `X-Late-Secret`           | Pre-shared secret. Constant-time compare. Rotated out-of-band.          |
| `X-Late-Fingerprint`      | User's SSH pubkey fingerprint (authenticated by bastion).               |
| `X-Late-Username`         | Optional hint; backend re-derives via DB lookup (fingerprint is authoritative). |
| `X-Late-Peer-IP`          | Required client IP, resolved from trusted PROXY v1 or a direct connection. Used for admission, session bookkeeping and IP bans. |
| `X-Late-Term`             | `$TERM` from `pty-req`.                                                 |
| `X-Late-Cols` / `X-Late-Rows` | Initial PTY size.                                                   |
| `X-Late-Session-Id`       | Bastion-minted UUIDv7, stable across reconnects (logs/metrics correlation). |
| `X-Late-Via`              | `bastion`. Forward-looking trail; eventually carries bastion version.   |
| `X-Late-Reconnect-Reason` | The numeric WS close code that triggered this redial (e.g. `4100`, `1006`). **Absent on first dial.** Presence == "this is a reconnect"; the value is also the reason. |

Backend validates secret + IP allowlist, looks up user by fingerprint, allocates the TUI session.

### Frames (post-handshake)

- **Binary frames**: raw PTY bytes, both directions. **No inspection, no escape-sequence parsing.**
- **Text frames** (JSON control vocabulary, intentionally tiny):
  - `{"t":"resize","cols":N,"rows":M}` — bastion → backend on SSH `window-change`.
  - `pty`, whitelisted `env`, then `shell_start` initialize each backend generation.
  - Bounded `exec_request` / `exec_response` support CLI token discovery before
    shell startup. Exec and shell reuse the same tunnel and stable session ID.
  - The canonical wire vocabulary lives in `late-core/src/tunnel_protocol.rs`.
- **Ping/pong**: WS control frames. Bastion pings every 2s; >5s of silence treated as a dead backend.

### Close codes — policy

The bastion does **not** interpret close-code semantics. It dispatches by numeric range:

| Code            | Bastion action                                                                 |
| --------------- | ------------------------------------------------------------------------------ |
| `1006`          | **Reconnect.** Pseudo-code; covers SIGKILL at end-of-grace and transient network drops. The dominant production reconnect path. |
| `4100`–`4199`   | **Reconnect.** Late-private retryable subspace — backend signals "user wants to reconnect." Currently allocated: `4100` = user pressed `r` in the upgrade dialog. |
| Anything else   | **Terminal.** Close the user's SSH session cleanly.                            |

Close codes the backend explicitly emits today (all Terminal):
- `4000` — user ended session (app-quit, or `q` in the upgrade dialog).
- `4001` — kicked.
- `4002` — auth revoked / banned / unknown user.
- `4003` — protocol error.

Other RFC 6455 / IANA codes (`1000`/`1001`/`1011`–`1014`/etc.) are not emitted by the backend — late-ssh never explicitly closes a healthy tunnel WS — and are treated as Terminal defensively. Reconnecting on an unknown signal risks looping into the same broken state.

The transport-error branch (tungstenite returns `Err` instead of a Close frame — TCP RST mid-stream) is dispatched the same as `1006`, since functionally that's what happened.

---

## 5. Bastion responsibilities (`late-bastion` crate)

Guiding principle: **the bastion is intentionally minimal.** No DB, no service deps, no protocol-aware logic over the byte stream.

What it does:
- Full SSH server (russh): pubkey auth, PTY, shell, resize, whitelisted env and
  bounded pre-shell exec. Reject extra interactive channels and unsupported requests.
- Host key via `load_or_generate_key` from a K8s Secret.
- PROXY v1 parsing on `:5222` (parser shared via `late-core`).
- One outbound WebSocket per open shell channel.
- **Pure byte pump in both directions; no inspection of the shell byte stream.**
- Translate SSH `window-change` requests into `resize` text frames.
- Global connection-count semaphore plus per-IP connection and attempt limits
  before SSH negotiation, keyed on the trusted PROXY client IP. Backend account
  admission remains independent.
- **On WS close**, dispatch by code per §4:
  - **`4100`** (user-requested reconnect): redial immediately and show
    "waiting for updated late.sh..." with a terminal reset.
  - **`1006`** (transport): reconnect with backoff. If reconnect is fast (<500ms), say nothing. Otherwise write "reconnecting to late.sh…" after `INITIAL_MESSAGE_DELAY` (with terminal reset to exit alt-screen), escalate to "still reconnecting…" after `ESCALATION_MESSAGE_DELAY`.
  - **Anything else** (Terminal): write a friendly closing message, close the SSH session.
- **On any redial**, send `X-Late-Reconnect-Reason: <numeric-code>` (the close code that triggered the redial; or `1006` if the previous pump ended on a transport error). Also `X-Late-Via: bastion` on every dial, fresh or redial.
- First connections and bootstrap waits show connecting progress after 500ms
  (bootstrap checks run on the ping cadence). Rejections and exhausted budgets
  print a closing explanation; backend HTTP 401/403 failures are logged at WARN.
- On SIGTERM: brief goodbye to all active SSH channels, close cleanly, exit. Single-replica means active sessions are dropped during bastion rollouts (acceptable per §1).
- No persistent state beyond per-session "this SSH channel maps to this backend URL + session id + current PTY size + last-close-code."

Each outage has one 30-second budget, including backoff, stalled handshakes and
upgrades that close without output. A backend generation counts as ready only
after binary output arrives; the five-second silence watchdog applies only
after that point and restarts after a successful SSH client write. Both output
legs bound blocked writes at 30 seconds. Retry delays use 50–100% jitter within
an exponentially increasing ceiling, without extending the outage budget.
On redial, discard queued keystrokes and paste;
retain the latest terminal size, term, whitelisted env, identity and session ID.
Input typed before replacement output is also discarded. Application state
restarts by design. Live bytes and resizes share an ordered, bounded FIFO;
sustained input stalls close the connection instead of silently losing events.
Output respects the remote SSH window and has bounded writes and shutdown.
Normal app quits send SSH exit status 0 before EOF/Close; kick, ban and other
error closes send status 1, so OpenSSH clients can distinguish normal exit.
Whitelisted terminal env values are bounded to 4 KiB each, with latest value
winning for repeated names.
Admission covers clients before PROXY parsing and SSH authentication, with a
10-second authentication deadline. Completed connection tasks are reaped.

What it does **not** do:
- No DB, no user-existence check, no ban check (late-ssh handles, replies with `4002`).
- Bastion limits pre-auth connections and attempts per IP; late-ssh independently
  retains its admission caps using `X-Late-Peer-IP`.
- No interpretation of, or coupling to, the TUI's terminal output.

---

## 6. Backend (`late-ssh`) changes

### `/tunnel` endpoint
- Second axum listener on `:4001` (ClusterIP-only Service).
- Validates `X-Late-Secret` (constant-time) and peer IP against an in-app allowlist.
- Looks up / creates user from `X-Late-Fingerprint`. Banned users get close code `4002`.
- Constructs `SessionConfig` and runs `App::new(...)` exactly as the russh path does.
- Forwards `resize` text frames to the same pty-resize path used by russh `window_change_request`.
- Reads `X-Late-Reconnect-Reason` and threads the numeric value (or `None` if absent) into the App context. The TUI uses it to drive the post-reconnect banner; nothing else branches on it.

### Tunnel-session lifecycle on SIGTERM
Late-ssh does not close a healthy tunnel WS at SIGTERM. Existing tunnel sessions
ride out the grace period, ending on user action (`4000`/`4100`) or transport loss
at process exit (bastion sees `1006`). Pre-shell and exec-only disconnects release
their registry entry and admission guards too. Old generations cannot unregister
a live replacement sharing their stable token or erase its paired-client state.

The /tunnel *acceptor* still stops accepting new connections at SIGTERM via axum's `with_graceful_shutdown`; this change is only about existing sessions.

### Upgrade-reconnect UX flow

1. **SIGTERM** → `is_draining = true` → banner overlay appears in all active sessions:
   ```
   ℹ️  Update available! Press q then r to get the late-est features!
   ```
2. **User presses `q` while `is_draining`**: the existing q-confirm dialog re-renders with upgrade-aware copy:
   ```
   ┌ Update? ─────────────────────────────────────────────────┐
   │                                                          │
   │                                                          │
   │         r to reconnect to the updated late.sh!           │
   │                                                          │
   │                                                          │
   │                                                          │
   │  q bye, I'll be back             Esc yeah, my bad, stay  │
   └──────────────────────────────────────────────────────────┘
   ```
3. **`r`** → app emits a graceful exit, /tunnel closes the WS with code `4100`. Bastion redials with `X-Late-Reconnect-Reason: 4100`. New backend renders the upgrade-welcome banner (below).
4. **`q`** → existing user-quit path; close code `4000`; bastion does a terminal close. User sees the existing plain-text farewell:
   ```
   Stay late. Code safe. ✨
   ```
5. **Esc** → dismiss the dialog; user stays on the old pod, banner remains, fate-of-grace-period applies.

The q-confirm dialog when **not** during drain is unchanged ("Quit? Clicked by mistake, right?").

### Post-reconnect banners (TUI side)

Rendered by `late-ssh/src/app/render.rs` toast pipeline.

**`X-Late-Reconnect-Reason: 4100`** — user explicitly chose to upgrade:
```
┌ Reconnected! ────────────────────────────────────────────┐
│                                                          │
│                                                          │
│       Welcome to the updated late.sh! Enjoy.             │
│                                                          │
│                                                          │
│                                                          │
│                                            Esc close     │
└──────────────────────────────────────────────────────────┘
```

**`X-Late-Reconnect-Reason: 1006`** — auto-reconnect after SIGKILL or transport drop:
```
┌ Reconnected! ────────────────────────────────────────────┐
│                                                          │
│                                                          │
│       You were reconnected to late.sh due to either      │
│       a software update or a network problem.            │
│       Either way, welcome back!                          │
│                                                          │
│                                                          │
│                                            Esc close     │
└──────────────────────────────────────────────────────────┘
```

**`X-Late-Reconnect-Reason` absent** — fresh connection, no banner.

A future PR (depends on Cargo workspace version bumping in CI) will replace the unified `1006` copy with an upgrade-specific or network-glitch-specific message based on a `last_connected_version` DB diff.

### What's unchanged
- Existing russh path on `:2222`. `late-cli` keeps using it directly.
- Per-IP rate limiter (key by authenticated `X-Late-Peer-IP` for `/tunnel`, by the trusted PROXY source or direct client address for russh).
- Session state continues to live only in the backend process and DB.
- The `Stay late. Code safe. ✨` farewell on `q`-driven disconnect.

---

## 7. Security model

The bastion is the trust root for user pubkey auth. The backend never sees user key material — only a fingerprint asserted by the bastion.

- **Layer 1 — Network isolation.** `:4001` Service is ClusterIP-only. NetworkPolicy allows ingress only from `late-bastion` pods.
- **Layer 2 — In-app IP allowlist.** `/tunnel` checks peer IP against a configured allowlist before accepting the upgrade. Reject 403 otherwise. Deliberately duplicative of Layer 1.
- **Layer 3 — Pre-shared secret.** `X-Late-Secret`, constant-time compare. Rotate by deploying new secret to backend first (accept old + new for overlap), then bastion.
- **Layer 4 — Transport.** Plain WS inside cluster is fine for MVP; `wss://` once bastion and backend cross networks.

Future (not v1): signed bastion identity (short-lived JWT) once we have >1 bastion or a key-rotation pain point.

**The backend does NOT trust:** the user's SSH pubkey (never sees it), any auth claim on the WS not backed by Layers 1–3, `X-Late-Username` (hint only), `X-Late-Reconnect-Reason` for anything other than driving the cosmetic banner (it's untrusted display data).

**The bastion does not inspect terminal bytes** — keeps it thin and decoupled from TUI versioning. **The bastion has no DB, no service deps, no per-user logic** — minimum surface for bugs that could force a bastion restart.

### Client IP attribution

Both SSH listeners require a usable source address when PROXY parsing is
enabled for a trusted transport peer. Missing, malformed and `PROXY UNKNOWN`
headers close the connection before SSH negotiation. A direct client uses its
socket address; an untrusted peer cannot override that address with a header.

After the backend validates the bastion's transport CIDR and shared secret,
`X-Late-Peer-IP` supplies the per-client attempt and connection limits,
`State.conn_counts` and `ActiveSession.peer_ip`. The latter supplies the IP
snapshot persisted by a server-ban command. Redial preserves the original
client IP; pairing does not replace it with the paired socket's address.
Kicks terminate the target account's sessions without creating an IP ban.
Existing bans on a genuinely shared client IP still apply to that IP.

IRC retains its rollout behavior: a trusted proxy may omit metadata or send
`PROXY UNKNOWN`, allowing account authentication with `peer_ip: None`. A
server ban then stores no IP unless another session of that account has a
known client address. IRC never substitutes its shared transport peer into
that snapshot. Unknown-IP authentication throttling may still use the
transport address as a network-protection key.

Infrastructure addresses remain available for transport trust, capacity
protection and diagnostics. Accepted tunnel logs label them
`transport_peer_ip`, separately from `client_ip`; they are not moderation
identity. Public IRC masks are unchanged.

---

## 8. Deployment

### Infra delta (`infra/`)
- **`infra/service-bastion.tf` (new)** — Deployment + Service for `late-bastion`, mirrors `service-ssh.tf`. Modest resources.
- **`infra/ssh-tcp.tf` (modify)** — Add the gated HAProxy `:5222` listener and `service_bastion` backend with `send-proxy` alongside the existing `:22` listener.
- **`infra/service-ssh.tf` (modify)** — Add container port `:4001` and a new `service-ssh-internal-sv` ClusterIP Service exposing only `:4001`.
- **NetworkPolicy** — allow ingress to `service-ssh-sv:4001` only from pods labeled `app=service-bastion`.
- **Secrets** — `BASTION_SHARED_SECRET` mounted into both pods; `BASTION_HOST_KEY` for the bastion's russh host key.

### Rolling backend deploy (post-cutover)
1. New `late-ssh` pod up; `/tunnel` healthy.
2. Old `late-ssh` pod gets SIGTERM → `is_draining = true`, banner appears in all active TUIs.
3. Engaged users press `r` (→ close `4100`, immediate reconnect) or `q` (→ close `4000`, terminal close). Non-engaged users keep using the old pod until k8s grace period expires.
4. SIGKILL → tunnel sessions see `1006` → bastion redials with `X-Late-Reconnect-Reason: 1006` → new backend shows the unified reconnect banner.

### Bastion upgrades
Rare. Plan for low-traffic hours; multi-replica is a future enhancement.

---

## 9. Migration strategy

Two paths run in parallel, controlled by which port the user dials.

| Phase | `:22`                          | `:5222`                        | Audience                                         |
| ----- | ------------------------------ | ------------------------------ | ------------------------------------------------ |
| 0     | → `service-ssh-sv:2222`        | (does not exist)              | All users.                                       |
| 1     | → `service-ssh-sv:2222`        | → `service-bastion-sv:5222`   | Default users on `:22`. Dogfooders on `:5222`.   |
| 2     | → `service-bastion-sv:5222`    | → `service-bastion-sv:5222`   | All users go through bastion. `:5222` is escape. |
| 3     | → `service-bastion-sv:5222`    | (removed)                      | Steady state.                                    |

`:22 → bastion` cutover is a one-line TF change. Rollback is the same line in reverse. `late-cli` continues to dial `service-ssh-sv:2222` directly until we choose to migrate it (see §11).

---

## 10. Phased implementation

Phases 1–4 are the chassis (largely done / in-flight on this branch). Phase 4.5 is the fit-and-finish work this PR rev adds. Phase 5 is the cutover.

### Phase 1 — Crate scaffold ✅
Workspace + russh skeleton + PROXY v1 parser lifted to `late-core`.

### Phase 2 — Backend `/tunnel` endpoint ✅
`:4001` axum listener, `/tunnel` WS handler, session bootstrap refactored to share between russh and `/tunnel`.

### Phase 3 — Bastion proxy logic ✅
Full byte pump end-to-end via `ssh -p 5222 late.sh`.

### Phase 4 — Reconnect loop ✅
Close-code dispatch, exponential backoff, plain-text reconnect message after grace.

### Phase 4.5 — Upgrade-reconnect UX (this PR rev)
- Add close code `4100` to the protocol; teach bastion's `classify_close_code` (only `1006` and `4100..=4199` Retryable).
- Bastion sets `X-Late-Reconnect-Reason: <numeric-code>` and `X-Late-Via: bastion` on redials. Drop the prior `X-Late-Reconnect` boolean header (the reason header subsumes it).
- Show "waiting for updated late.sh..." for `4100` redials (immediate redial).
- `/tunnel` reads `X-Late-Reconnect-Reason`, threads `Option<u16>` into `App` context.
- Remove the `shutdown.cancelled() => emit close 1000` arm from `late-ssh/src/tunnel.rs:520-536`. Existing tunnel sessions now ride out the grace period like russh; no late-ssh-emitted close on healthy sessions.
- Modify the q-confirm dialog (§6) to render upgrade-aware copy when `is_draining`. `r` emits `4100`, `q` emits `4000`, Esc dismisses.
- Update the draining banner copy (§6).
- Post-reconnect banner in `render.rs` keyed on the relayed close code (only `4100` and `1006` are reachable in this PR scope).
- Tests cover retry classification, stable identity, upgrade notices, stale input,
  lifecycle cleanup and the real bastion/backend chain. See
  [BASTION-QUALIFICATION.md](BASTION-QUALIFICATION.md) for the packaged campaign.

### Phase 5 — Production cutover
- Soak window with both paths up.
- Decide and ship `late-cli`'s post-cutover routing (see §11).
- Flip `:22` TF entry to `service-bastion-sv:5222`. Observe; rollback in-place if needed.

### Phase 6 — Nice-to-haves (future)
- Multi-replica bastion.
- Metrics: reconnect count + duration, close-code distribution, handshake latency, bytes pumped.
- `last_connected_version`-driven post-reconnect copy (separate PR — depends on Cargo workspace version bumping in CI).
- Additional platform-specific CLI media checks.

---

## 11. Open questions

- **`late-cli` routing after `:22` cutover (Phase 5).** CLI token discovery now
  runs through the bastion's pre-shell exec, and pairing reuses its stable token
  across backend replacements. The production port cutover remains a separate
  infrastructure action, gated on staging checks.
- **Tailored "what's new" welcome.** Deferred. Requires Cargo workspace version bumping in CI (current crates are pinned at `0.1.0` and never bumped). Tracked separately; Phase 4.5 lays the substrate (`X-Late-Reconnect-Reason` is the per-redial signal), but the version-aware copy is follow-up scope.

---

## 12. What we explicitly considered and rejected

- **Telnet between bastion and backend.** IAC byte-stuffing requires data-stream inspection — contradicts the bastion's "thin / opaque pump" property.
- **Raw TCP with bespoke framing.** Reinvents WebSocket badly.
- **`/tunnel` on existing public `:4000`.** Removes kernel-level isolation between public API and trust-the-headers tunnel. Footgun.
- **Big-bang `:22` cutover.** Replaced by parallel-path rollout.
- **Full SSH-in-SSH proxy** (bastion as russh server + russh client). Doubles SSH framing/crypto on the inner hop and forces exec-request gymnastics for identity passing.
- **Bastion holding per-user / ban-aware state.** Maximum bastion uptime > convenience;
  the only per-IP state is bounded-lifetime transport admission accounting.
- **Bastion-side text prompt on close 1000** (alternative to TUI dialog). Considered as the simplest way to give users a choice. Rejected: constrained to plain-text rendering, can't reuse the q-dialog pattern users already know, and pairs awkwardly with the post-reconnect banner. The TUI-side dialog uses the existing modal surface and pairs naturally with the banner.
- **Second SIGTERM as reaper signal** (so late-ssh could distinguish "rode out grace period" from "transport blip" on auto-reconnects). Rejected: requires non-standard k8s configuration (preStop hook or sidecar timer) and a per-pod SIGTERM counter, with marginal benefit. The `last_connected_version` follow-up gives finer-grained welcome-vs-apology routing without the infra cost.
- **Two reconnect headers (`X-Late-Reconnect: 1` + `X-Late-Reconnect-Reason: enum`).** Rejected for the simpler one-header form: `X-Late-Reconnect-Reason: <close-code>`, presence ⇒ reconnect, absence ⇒ fresh. The numeric close code is sufficient for both signals.
- **All `1xxx` codes Terminal.** Considered as the maximum-simplicity option (one-line policy: `4100..=4199 ⇒ Retryable, _ ⇒ Terminal`). Rejected because it breaks the SIGKILL path: at end-of-grace-period, k8s sends SIGKILL → tunnel sessions see `1006` → all-1xxx-Terminal would drop them. Carving `1006` out as Retryable preserves the bastion's flagship feature (the dominant production reconnect path) at the cost of one extra match arm. Other `1xxx` codes are not emitted by the backend in our protocol and are treated as Terminal defensively.
- **Always show the unified reconnect banner, including on `4100`.** Considered for further simplification. Rejected: the `4100` case is qualitatively different — the user **chose** to upgrade and deserves a clean upgrade-positive welcome, not a "something happened, sorry" hedge. One extra match arm; clearly worth it.
- **Backend skipping splash/welcome on reconnect.** Considered; rejected for MVP. The reconnect banner is the only reconnect-aware UI; existing splash/welcome path is unchanged.

---

## 13. Remaining review follow-ups

Finding numbers refer to the pre-merge review. These items remain open after
the fixes through `29829d29`; documenting them does not establish production
qualification.

### #10 — Backend liveness and cleanup

The backend does not initiate tunnel heartbeats or enforce a receive-idle
deadline (`late-ssh/src/tunnel.rs`). If bastion disappears without closing its
socket, session accounting can remain until a transport error or a later write
failure. An indefinite leak has not been demonstrated; the output-write timeout
only bounds a write that actually blocks.

Before public rollout, test a silent network blackhole and establish a bounded
cleanup deadline. Add backend liveness detection as needed, preserving startup
and slow-client tolerances. Verify that active users, pairing routes, per-IP
counts and global permits return to baseline, and that stale cleanup cannot
retire a healthy replacement using the same session token.

### #12 — TCP probes produce misleading warnings

TCP health probes reaching a trusted PROXY listener close without sending a
PROXY header. The accept loop logs those expected failures at WARN
(`late-bastion/src/ssh.rs`). Provide a suitable probe endpoint or distinguish
empty probe disconnects from malformed headers. Verify readiness/liveness still
work and genuine PROXY failures remain diagnosable.

### #13 — Bastion telemetry is not connected

`late-bastion/src/main.rs` initializes plain tracing rather than the shared OTLP
pipeline. The `otel` feature and collector environment variable alone do not
enable export, and the bastion egress policy permits only DNS and backend port
4001, excluding collector port 4317.

Initialize telemetry, allow collector egress, and flush providers on shutdown.
Add the reconnect, close-code, handshake and byte metrics listed in Phase 6.
Verify collector receipt with `otel` enabled and retain a working build without
that feature.

### #14 — Define the protocol version-skew contract

`ControlFrame` rejects unknown `t` tags; the backend closes unknown control
frames with 4003. The protocol comment makes compatibility conditional on
tolerance, rather than promising that tolerance already exists. A new variant
alone does not break existing traffic; sending an unsupported message can.

Document supported version combinations and deployment order before extending
the wire vocabulary. Test mixed bastion/backend versions. Ignore unknown
messages only where their semantics permit it; mandatory setup or control
operations need an explicit compatibility strategy.

### #15 — Remaining config and documentation cleanup

- Align the bastion's 7200-second termination grace and its comment with the
  actual SIGTERM behavior: active SSH connections are cancelled, rather than
  retained for a two-hour drain.
- Replace the stale "NGINX TCP entry" wording in `infra/variables.tf` with the
  current `ssh-edge` HAProxy terminology.
- Reconcile `BASTION-REDUX-REVIEW.md` references to the removed `deploy.yml`
  workflow and `BASTION_TUNNEL_TRUSTED_CIDRS` variable with current code.

### Deferred output-budget parity

The scoped #4 fix aligns blocked output writes at 30 seconds and prevents
client backpressure from triggering the backend watchdog. Tunnel rendering
still uses `budget: None` with a bounded output queue; it does not reproduce
direct SSH's 32-MiB backlog accounting and render-pause behavior. Exact parity
remains separate follow-up scope, requiring slow-reader memory and recovery
tests if pursued.

### Deferred IPv6 HTTP and pairing attribution

`infra/ipv6-proxy.tf` forwards HTTP(S) over TCP without client-address metadata.
That configuration can make ingress report the proxy address to the paired
WebSocket endpoint's IP-based limits. HTTP pairing addresses do not supply
`ActiveSession.peer_ip` or server-ban snapshots. Live deployment behavior has
not been checked as part of this work.

Preserving IPv6 HTTP client addresses requires a separate ingress design,
deployment order and rollback plan, with trusted-header and per-client pairing
limit tests. Keep that infrastructure change separate from the SSH/IRC
moderation attribution contract.

### Packaged and live rollout verification

A fresh [packaged smoke test](BASTION-QUALIFICATION.md#review-fix-verification)
at `a912f086` passed with 100 clients and two backend replacements, using
release images built with `otel`. The earlier long soak still predates these
review fixes. Complete the
[remaining staging checks](BASTION-QUALIFICATION.md#limits-and-remaining-deployment-checks)
before public-port cutover: Kubernetes endpoint replacement, PROXY source IP,
NetworkPolicy including IRC, mounted secrets, pairing, terminal cleanup and the
rollback route.
