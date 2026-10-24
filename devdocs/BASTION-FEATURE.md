# Bastion: current design

The bastion keeps an SSH connection open while `late-ssh` is replaced. The
application starts a fresh session on the replacement backend; in-app state is
not restored. The first production switch from direct SSH to bastion requires
the approved [out-of-service maintenance cutover](BASTION-DEPLOY-GUIDE.md).
The [QA record](BASTION-QA.md) distinguishes local evidence from live cluster
checks that remain to be done.

## Request paths

```text
SSH/late CLI -> ssh-edge :22 (PROXY v1) -> late-bastion :5222
             -> private WebSocket /tunnel -> late-ssh TUI
late CLI and webview -> HTTPS /api/ws/pair -> late-ssh paired session
IRC -> ssh-edge :6697 (PROXY v1) -> late-ssh ircd
HTTP(S) IPv4/IPv6 -> host-network ingress-nginx -> late-web / late-ssh API
```

The production `BASTION_ENABLED` flag selects the public SSH destination:
`0` routes to direct `late-ssh:2222`; `1` routes to `late-bastion`. IRC remains
on its own route. The CLI's default `late.sh:22` SSH connection follows that
same public path, including its pre-shell token lookup. Local development can
still reach the backend directly on `:2222`. The CLI's pair WebSocket is a
separate HTTPS connection; it retries across backend replacement and requires
a currently live session for its token. The public `/listen` page is unpaired.
CLI audio plays on the user's device independently of the TUI; visualizer and
control messages pause while the pair socket reconnects. Live external music
and device behavior has not been qualified by the local campaign.

`late-bastion` authenticates the SSH public key, handles PTY, resize and
whitelisted environment requests, and tunnels shell bytes without parsing
terminal output. It also supports the CLI's bounded pre-shell exec request.
The bastion has no database, account or ban logic. `late-ssh` resolves the
fingerprint, enforces account admission and bans, owns the TUI and pairing
registries, and serves `/tunnel` on a separate private listener. Production
uses `late-ssh:4001` for that listener and `:4000` for the public API; dev
profiles use `:4002` for the tunnel.

## Tunnel and replacement contract

The bastion opens `/tunnel` with a shared secret, authenticated key
fingerprint, original client IP, stable UUIDv7 session ID, PTY information,
and a reconnect reason only on redial. The backend accepts this metadata only
from an allowed transport peer with the correct secret. The wire vocabulary is
defined in `late-core/src/tunnel_protocol.rs`: JSON text frames set up PTY,
terminal environment, ordered resize and one bounded exec request/response;
binary frames carry opaque shell bytes after `shell_start`. Exec has no stdin.
The stable session ID also acts as a pairing bearer token; logs correlate it
by hash rather than exposing the full value.

An app quit closes with `4000`, a kick with `4001`, and revoked authentication
or a ban with `4002`; those end the SSH session. `4100` (the user's upgrade
choice), other `4100`–`4199` closes, abnormal loss (`1006`), and transient
transport or HTTP 5xx failures trigger redial. Other closes are terminal.
The backend retains existing tunnel sessions during its SIGTERM grace period;
the user can reconnect with `r`, quit with `q`, or remain on the old backend
until it exits. A forced backend loss reconnects automatically. The TUI shows
the reason after reconnect.

The bastion has a 30-second outage budget across backoff and handshake
attempts. A replacement is ready only after it emits binary output. During an
outage, queued input is discarded, including input before that first output;
the latest terminal size, allowed environment, identity, session ID and client
IP survive. Input and resize ordering is preserved while connected. Blocked
writes have deadlines, and slow clients cannot hold a retiring session
indefinitely. Old backend cleanup cannot remove a replacement's route or
paired state for the same stable token. A bastion rollout itself disconnects
active SSH clients: there is one replica and no bastion handoff.

## Client IP and trust boundaries

`ssh-edge` forwards the real client address in PROXY v1 for SSH. IRC does so
when `IRC_PROXY_EMIT=1`; otherwise its trusted-proxy path may lack a usable
client address.
The bastion accepts that header only from configured trusted peers and
rejects missing, malformed or unknown SSH client addresses before SSH
negotiation. Direct backend SSH behind `ssh-edge` also requires a usable
trusted PROXY address; local direct connections without that proxy use the
socket address. After the private tunnel's network and secret checks,
`late-ssh` uses the asserted
`X-Late-Peer-IP` for per-client limits, active sessions and server-ban IP
snapshots. Redial keeps that same address. The proxy or bastion pod's address
is retained separately for transport trust and diagnostics; it is never a
moderation identity. Kicks target the account's sessions without creating an
IP ban.

IRC may accept a trusted proxy connection with no usable client address. Its
session then has `peer_ip: None`; a server ban stores no IP from that session
unless another session of the account has a known client address. It never
substitutes the shared infrastructure peer. Unknown-IP rate protection may
still use the transport address.

For HTTP(S), ingress-nginx binds the node's IPv4 and IPv6 interfaces directly.
It ignores untrusted incoming forwarded headers and writes the socket peer
into `X-Forwarded-For`; the API accepts that value only from its trusted
ingress. Pairing attempt and socket limits therefore apply per HTTP client
address. HTTP pairing addresses do not overwrite SSH/IRC moderation identity.
The old IPv6-only TCP proxy is retired during the first cutover because it
hid IPv6 client addresses from HTTP and pairing.

The private `/tunnel` Service is ClusterIP-only. Its NetworkPolicy admits the
bastion pods, and `late-ssh` additionally checks the source CIDR and shared
secret. The fingerprint is authoritative; a forwarded username is a hint.

## Implementation map

| Area | Primary touchpoints |
| --- | --- |
| SSH frontend | `late-bastion/src/ssh.rs`, `proxy.rs`, `admission.rs`, `config.rs` |
| Shared wire | `late-core/src/tunnel_protocol.rs` |
| Edge client IP parsing | `late-core/src/proxy_protocol.rs` (bastion), `late-ssh/src/proxy_protocol.rs` (direct SSH/IRC) |
| Backend tunnel and sessions | `late-ssh/src/tunnel.rs`, `session_bootstrap.rs`, `session.rs`, `paired_clients.rs` |
| Moderation and IRC | `late-ssh/src/moderation/`, `ircd/` |
| CLI pairing | `late-cli/src/ssh.rs`, `ws.rs`; `late-webview/` |
| Infrastructure | `infra/ssh-tcp.tf`, `service-bastion.tf`, `service-ssh.tf`, `ingress.tf`, `ipv6-proxy-retirement.tf`, `network-policies.tf` |
| Release routing | `.github/workflows/release.yml`, `deploy_service.yml`, `deploy_infra.yml` |

## Open work

- **Before public cutover:** test a silent bastion/backend network blackhole
  and establish a bounded backend cleanup deadline. The backend has no
  application heartbeat or receive-idle timeout; if cleanup needs one, verify
  session, pair route, per-IP count and global permit release without retiring
  a healthy replacement. The live cluster, ingress, IP and rollback checks in
  the [deploy guide](BASTION-DEPLOY-GUIDE.md) also remain gates.
- TCP health probes disconnect before a PROXY header and currently create
  misleading WARN logs. Distinguish probes from malformed client headers or
  use a suitable health endpoint while preserving useful failure diagnostics.
- Bastion tracing is not connected to OTLP and its egress policy lacks the
  collector route. Add export, shutdown flush and reconnect/close/handshake
  metrics, then verify collector receipt.
- Define the mixed-version wire contract before adding control frames.
  Unknown `t` tags currently fail parsing and close the backend tunnel with
  `4003`; deploy order and version tests must cover any extension.
- `infra/service-bastion.tf` requests a 7200-second termination grace, but
  bastion SIGTERM closes active SSH connections. Revisit that duration; it
  provides no session drain.
- Tunnel output uses a bounded queue and write deadline but lacks direct
  SSH's 32 MiB render-backlog budget. Check slow-reader memory and recovery
  before pursuing exact output-budget parity.
- Multi-replica bastion, version-aware reconnect copy, bounded stdin/binary
  exec extensions, and uninterrupted pairing sockets through backend gaps
  are possible future work. Broader CLI media device qualification remains
  outside the current [local QA evidence](BASTION-QA.md).
