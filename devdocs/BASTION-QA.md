# Bastion qualification

Local pre-deployment evidence, beginning 2026-10-22, on
`mevanlc--bastion-redux` at `ee492675` plus the working-tree repairs described
below, subsequently committed in `04e56d36`. The later review fixes have a
separate [verification record](#review-fix-verification). The later client-IP
and disposable ingress checks are recorded below. None of these checks was a
production deployment; live cutover gates are in the
[deployment guide](BASTION-DEPLOY-GUIDE.md).

## Reproduce

Requires Rust/nextest, Docker Compose and uv. From the repository root:

```sh
make test-bastion CHECK_BUILD_JOBS=4
make test-bastion-repeat CHECK_BUILD_JOBS=4
make test-bastion-stack
make test-bastion-soak
```

`test-bastion` owns a temporary PostgreSQL project on port 55437. Override
`BASTION_CHECK_INSTANCE` and `BASTION_CHECK_PG_HOST_PORT` for concurrent runs.
`test-bastion-repeat` runs five lifecycle cases 100 times without retries.
The image targets build both release services with telemetry enabled; amd64 is
the default because the Dockerfile's door assets require that platform.

To run existing qualification images with custom load:

```sh
uv run scripts/test/bastion_qualification.py --clients 100 --duration 120 --faults 2
```

The packaged runner creates a unique Compose project, dummy credentials,
disposable SSH keys, a tmpfs database and blue/green backends behind HAProxy.
An explicit empty Compose env file prevents loading checkout `.env` files.
Only HAProxy joins the access network; application containers use an internal
network. Published ports bind loopback. There are no cloud credentials.
Normal completion, failure, Ctrl+C and SIGTERM collect evidence and remove the
owned containers/volumes. Abrupt host/daemon failure can still require removing
the project named in `endpoints.json`.

Reports live under ignored `target/bastion-qualification/<timestamp>-<id>/`:
`summary.json`, image IDs, endpoint metadata, resource samples and container
logs. Failure reports also retain terminal output. Temporary client private keys
are removed after the run. Do not add generated keys or full logs to Git.

## Repairs exposed by the campaign

| Problem | Resulting behavior and regression evidence |
| --- | --- |
| Exec-only and pre-shell tunnel disconnects retained a writer sender, registry route and admission guards | Dropping pre-shell channels and bounding writer teardown releases every count/permit. The new disconnect regression failed before the repair. |
| An old tunnel/App could unregister a live replacement sharing its stable token | Retirement checks the receiver under the registry write lock; paired cleanup occurs under that same lock. Physical active-session entries are removed one at a time. The replacement regression failed before the repair. |
| Stalled upgrades and empty successful upgrades could extend an outage | Dial/write waits are bounded and backoff shares an outage budget; a backend must emit binary output before recovery is considered successful. Both stalled-handshake and repeated-empty-upgrade regressions finish at approximately 30 seconds. |
| Queued paste/keystrokes could reach a newly reset app; full input queues could silently lose events | Redial consumes stale input, retains the latest size, and waits for replacement output before forwarding new input. Live bytes and resizes retain FIFO ordering; sustained stalls end the session. |
| Pre-auth clients escaped effective admission bounds, completed tasks accumulated, and listener failure left the process alive | Admission precedes PROXY parsing/SSH setup, authentication has a deadline, completed tasks are reaped, and startup errors exit nonzero. Saturation, expiry, shutdown and packaged startup checks cover these paths. |
| Concurrent cold backend startup raced on `_migrations` creation | A dedicated connection holds a session advisory lock over migration startup. Closing it on success/error/cancellation releases the lock. The cold-start regression reproduced the PostgreSQL catalog error before the repair; both packaged replicas now start together. |
| An invalid explicit CLI config silently fell back to the default destination | `--config` read/parse errors now stop startup. A regression failed before the repair and passes afterward. |
| Repeated terminal env requests accumulated duplicate/stale values | Whitelisted names retain one latest value, capped at 4 KiB. The real SSH setup regression fails on the old behavior and passes with duplicate replacement/oversize rejection. |
| A normal shell quit lacked an SSH exit status, so OpenSSH mode exited 255 | The bastion sends exit status before EOF/Close: normal app quits succeed, kick/ban closes fail. The regression failed before the repair; real CLI exit/terminal checks verify the resulting behavior. |

The reconnect contract preserves SSH transport, account/key identity, stable
session token, terminal type, latest dimensions and whitelisted env. The app
restarts. Input received during the outage or before replacement output is
discarded; in-app state restoration is outside this contract.

## Results

| Check | Observed result |
| --- | --- |
| Final Linux workspace, all targets, `--features otel` | 5,824 passed, 17 skipped. Skipped tests remain skipped, including opt-in arenas. |
| Full Linux strict Clippy, all targets, telemetry | Passed with `-D warnings`. |
| Final native bastion suite | 39 passed, including env bounds, SSH exit status and both outage-budget cases. |
| Native targeted Make target | Final 86 passed; temporary database removed by the target. |
| Native CLI suite | 102 passed. |
| Migration failure/release and cold concurrency | Both passed against disposable PostgreSQL. |
| Five high-risk regressions × 100 iterations | 500 executions passed; 100/100 iterations, no retry masking. |
| Final env/exit-status cases × 100 iterations | 200 additional executions passed; 100/100 iterations. |
| Packaged 100-client smoke, 120 seconds, two replacements | Passed; graceful max recovery 2.561 s, kill max 1.424 s. |
| Packaged 45-minute soak, 100 clients, ten replacements | Passed: all 100 original SSH connections survived all ten replacements. |
| Final image, 100 clients, ten minutes/four replacements | Passed with no SSH disconnects. Includes the env and exit-status repairs added during the long soak. |
| Packaged harness cleanup | Four-client/two-replacement run passed; after disconnect bastion had 11 FDs, backend 31. SIGTERM cancellation removed all owned containers and client keys. |
| Real terminal | OpenSSH navigation, mouse selection, Unicode input/paste and resize passed. Final native/old/openssh CLI modes retained their processes through requested upgrade and kill, accepted Unicode input afterward, exited 0 and restored exact termios attributes and mouse mode. |
| Packaged host identity/startup | Restart preserved the read-only host key and SSH fingerprint. Unwritable key path exited nonzero in approximately one second. |
| Static infra | Terraform recursive format and backend-free init/validate passed. Actionlint syntax passed; full ShellCheck mode reports the existing SC2129 style warning in `.github/workflows/ci.yml:218`. |

The repeat cases cover stale input/latest size, exec-only cleanup, overlapping
token cleanup, registry retirement, and the real SSH → bastion → database-backed
backend reconnect. Socket tests also cover PROXY source IP/trust, malformed and
fragmented headers, data/resize/data order, opaque large paste, env filtering,
exec→shell reuse, extra-channel rejection, authentication expiry, saturation,
terminal vs retryable closes, heartbeat silence and zero-window shutdown.

The soak uses 80 idle clients and 20 active clients typing unique markers into
the actual TUI composer. It alternates five SIGTERM/drain/user-upgrade events
with five SIGKILL events. Every client must enter the replacement app without
losing its original SSH connection; paired WebSocket token routing is checked
after every replacement. Account provisioning exercises exec-only cleanup;
returning accounts bypass the first-visit tour so input probes reach the
composer. Fresh-account startup has separate Rust integration coverage.
Each replacement target becomes healthy before edge cutover and fault injection,
as in a rolling deployment. Stalled upgrades, HTTP rejection and longer outages
have separate socket regressions; the load run does not measure cold recovery
with no healthy backend available.

The same backend supplies a direct-SSH first-frame baseline. First-frame,
input-to-render and recovery distributions, CPU/memory samples and process FD
counts are retained. CPU and timing are local measurements on an ARM Mac with
amd64 containers under OrbStack, with concurrent build/test activity; they are
not production throughput estimates. The bastion has a 1-CPU/512-MiB limit.

The final-image run recorded 900 input probes: median 20.7 ms, p95 37.0 ms,
p99 41.9 ms, maximum 44.3 ms. Across 400 client recoveries, p95 was 2.96 s and
maximum 3.33 s. After disconnect, the bastion returned to 11 FDs/two threads
and the active backend to 31 FDs/eight threads. The single direct first-frame
sample was 233 ms; the ramped bastion median/p95 were 143/164 ms. Different
warm-up histories prevent interpreting that as a speed improvement.

The 45-minute run recorded 4,120 input probes (p95 39.6 ms, maximum 63.3 ms)
and 1,000 client recoveries (p95 2.71 s, maximum 3.12 s). Its 206 Docker samples
show bastion memory at 34.0 MiB initially, 43.8 MiB finally, and 46.4 MiB peak;
median/peak sampled CPU were 3.31%/15.29% of one core. Process samples held at
211 FDs after the temporary CLI sessions left. These are measured bounds for
this workload, rather than a claim about arbitrary load or indefinite lifetime.

## Evidence

All paths below are relative to the checkout and intentionally ignored:

- Full Linux test log: `target/bastion-linux-full-suite-final.log`.
- Final strict Clippy log: `target/bastion-linux-clippy-final.log`.
- Repeat log: `target/bastion-repeat-100.log`.
- Final env/exit-status repeat log: `target/bastion-repeat-final-100.log`.
- Targeted Make log: `target/bastion-make-target.log`.
- 100-client smoke: `target/bastion-qualification/20261022-112011-2edc6c/`.
- 45-minute campaign: `target/bastion-qualification/20261022-112828-74b908/`.
- Final harness smoke: `target/bastion-qualification/20261022-114313-b70db6/`.
- Final server image campaign: `target/bastion-qualification/20261022-115540-aa0c05/`.
- Deliberately cancelled cleanup test: `target/bastion-qualification/20261022-114314-aeb866/`.
- Terminal captures/logs: `target/bastion-qualification/tty-*.png`, `cli-*.log`.
- Final CLI assertions: `target/bastion-qualification/tty-final-results.json`.
- Startup result: `target/bastion-startup-failure.json`.
- Reconstructable source diff: `target/bastion-qualification-source.patch`.

Packaged server image IDs:

```text
late-bastion  sha256:4d310c46f9540e0611597acb8490249a4f5548849cd0081e9baa76d32cb6c034
late-ssh      sha256:3ca14609deeec42fbe41b0ab38fedf08ebd6e86adecaf4718bc6b35aab4fa96c
final bastion sha256:4b09321a7dd3b91704a0c8cd08e1545a257e9737df7860fb8ebc2f69cd3afba0
```

The 45-minute campaign predates the final env-storage bound and shell exit-status
repair. The ten-minute final-image campaign and final CLI checks exercise those
changes; all other server lifecycle repairs are shared by both images.

## Review-fix verification

On 2026-10-22, rebuilt the release images from clean revision `a912f086`,
including all review fixes through `29829d29`, and ran:

```sh
make bastion-test-images
uv run scripts/test/bastion_qualification.py --clients 100 --duration 120 --faults 2
```

Both image builds passed with `--features otel`. All 100 original SSH
connections survived a graceful backend replacement and a SIGKILL replacement.
Pairing round trips passed before replacement and after each fault.

| Check | Observed result |
| --- | --- |
| Graceful replacement, 100 recoveries | Maximum 2.461 s. |
| SIGKILL replacement, 100 recoveries | Maximum 1.230 s. |
| 180 input probes across 20 active clients | Median 31.6 ms, p95 51.6 ms, maximum 69.5 ms. |
| After clients disconnected | Bastion: 11 FDs/two threads; active backend: 31 FDs/ten threads. |
| Cleanup | Owned containers, networks, volumes and client private keys removed. |

Evidence is retained under
`target/bastion-qualification/20261022-144313-0e401f/`; the image build log is
`target/bastion-review-images.log`. Image IDs:

```text
late-bastion sha256:6b96cd80ebc0f5c3d0292493d7370515f3e2951a8d81ce861a002336466f8c40
late-ssh     sha256:da5eb3656608356cd206fdb0048e9b21ec63967ee33b503b95284c91c638eabc
```

This two-minute smoke refreshes packaged reconnect evidence for the review
fixes; the longer soak and full workspace results above still describe their
earlier snapshots. The fixture uses elevated admission limits and supplies a
distinct synthetic PROXY source address per client. The run does not establish
production admission defaults, OTLP export, silent-blackhole cleanup or the
staging checks below.

## Client-IP moderation verification

Verified clean revision `be9f6721` on 2026-10-22, including the upstream merge
at `35f7c627` and the client-attribution fix.

Before the listener fix, both new `PROXY UNKNOWN` regressions failed: bastion
and direct SSH each sent a 22-byte SSH banner instead of dropping the
connection. After the fix, all **172 focused tests passed with `otel` enabled**,
covering the PROXY parsers, both SSH listeners, tunnel admission, IRC and server
moderation. Commit hooks also passed formatting and strict Clippy.

The real SSH → bastion → database-backed backend tests use separate client IPs
through shared infrastructure. Actual kick and ban commands verify that the
ban row captures the client's address, unrelated clients remain connected and
can reconnect, kicks allow the target to return, account bans survive an IP
change, and a fresh identity cannot evade an IP ban. IPv4 and IPv6 metadata are
covered. Separate tests verify independent per-client attempt/connection limits,
resource release after rejected metadata, and rejection of untrusted assertions.
Backend replacement and a real paired WebSocket preserve the original session IP.

IRC socket tests exercise the same moderation commands with known IPv4/IPv6
addresses, `PROXY UNKNOWN`, and omitted metadata. Unknown-IP bans store SQL
`NULL`, the target account remains banned, and other clients through the proxy
remain usable. These tests use local IPv4 sockets with synthetic PROXY metadata
and plaintext IRC; native IPv6 transport and production TLS remain staging checks.

To repeat the attribution-specific cases from the larger focused run on Linux:

```sh
make test-llm ARGS="-p late-bastion -p late-ssh --lib --features otel -E 'test(client_ip) or test(bastion_chain_transport_loss)'"
```

Rebuilt both release images with `otel` and repeated the 100-client, 120-second,
two-replacement campaign. All 100 original SSH connections survived; pairing
passed before replacement and after each fault.

| Check | Observed result |
| --- | --- |
| Graceful replacement, 100 recoveries | Maximum 2.373 s. |
| SIGKILL replacement, 100 recoveries | Maximum 1.236 s. |
| 180 input probes across 20 active clients | Median 26.9 ms, p95 38.0 ms, maximum 46.6 ms. |
| Accepted-tunnel log audit | 400 handshakes: 200 fresh, 100 graceful redials, 100 abnormal redials. |
| Client-IP attribution through HAProxy | All 100 synthetic client addresses preserved in every phase, stable per key and session; transport peer was separately recorded as `192.168.97.6`. |
| After clients disconnected | Bastion: 11 FDs/two threads; active backend: 31 FDs/ten threads. |
| Cleanup | Owned test databases, containers, networks, temporary checkout and client private keys removed. |

Evidence:

- Before-fix failures: `target/client-ip-before.log`.
- Final source suite: `target/client-ip-focused.log`.
- Release builds: `target/client-ip-image-build.log`.
- Packaged campaign: `target/bastion-qualification/20261022-172905-2b4f3b/`,
  including `summary.json` and `client-ip-attribution.json`.

```text
late-bastion sha256:a685e4a6e8f7bee2012ba268c1746703a668ab72d858324fe179753012be3d36
late-ssh     sha256:145d24c11409f9a5abebceffba55b1301a2e063930c07cfbaa9bac74c871a83d
```

The moderation assertions are from the source integration tests; the packaged
campaign validates forwarding, replacement and pairing with the actual HAProxy
process. The earlier full-workspace and long-soak results describe older
snapshots. Native IPv6 HTTP ingress and pairing attribution are prepared for
the maintenance cutover; their live checks are in the
[deployment guide](BASTION-DEPLOY-GUIDE.md).

## Disposable dual-stack ingress check

On 2026-10-23, a disposable Kind cluster ran ingress-nginx chart 4.15.1 with
host networking on, hostPort off, and forwarded-header trust disabled. IPv6
clients `::101` and `::102` and IPv4 client `192.168.117.101` arrived at the
fixture backend with distinct socket-derived forwarding addresses. A forged
client `X-Forwarded-For` was overwritten. HTTPS on IPv6 succeeded; an HTTPS
WebSocket upgrade returned `101` and delivered IPv6 client `::104` to the
backend. The fixture closed that socket immediately, so this did not test a
sustained paired session or pairing limits. The cluster and fixtures were
removed; the machine's built-in Kubernetes environment was not changed.
This verifies the ingress configuration in Kind, not the production RKE2
chart, CNI, node listeners or deployed pairing path.

## Limits and remaining deployment checks

Local HAProxy replacement and the disposable Kind check do not prove real
Kubernetes Service endpoint changes, HAProxy PROXY handling, CNI NetworkPolicy
enforcement, production Secret mounts or ingress routing of paired clients.
During the blocked maintenance window, verify those components, the known
public client IP over both IPv4 and IPv6 (including TLS IRC), pairing,
terminal cleanup and the rollback route before reopening traffic. A silent
network-blackhole cleanup check remains a pre-cutover item in the
[feature doc](BASTION-FEATURE.md#open-work). No production Terraform plan/apply
or cluster changes were performed here.

This campaign does not qualify external music, voice, video, clipboard hardware,
door services, platform-specific terminals or restoration of application state.
The CLI checks used local endpoints and a muted test account; local audio was
allowed to remain disabled when its fixture stream was unavailable. Their unit
tests pass, but that is separate from live device/service qualification.

During initial CLI fixture setup, a malformed temporary config was ignored and
three launches reached the default public endpoint. They were stopped without
TUI input; an orphan OpenSSH control master was also closed during process
cleanup. Login/presence side effects are possible. The explicit-config
repair above prevents that fallback, and subsequent launches supplied explicit
local routing flags as well as the corrected config.
