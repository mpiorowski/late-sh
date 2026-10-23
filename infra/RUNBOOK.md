# late.sh production runbook

Operating the live cluster: database access, triage, deploys, and the incident
log. Cluster setup, nodes and Terraform variables are in `infra/README.md`;
capacity findings and the scaling plan are in `SCALE.md`.

## Edge facts

- **SSH and IRC (`ssh-edge`).** Public `22` and `6697`, IPv4 and IPv6, belong to
  the `kube-system/ssh-edge` HAProxy DaemonSet (`infra/ssh-tcp.tf`, host network
  on `server-1`), which forwards raw TCP with PROXY v1 to `service-ssh-sv` or,
  when `BASTION_ENABLED=1`, SSH to `service-bastion-sv` (IRC sends PROXY v1 only
  when `IRC_PROXY_EMIT` is on). The backend sees the node address
  `46.62.210.86` as the transport peer and, when present, the PROXY address
  as the user IP. Never put these ports back on
  ingress-nginx: nginx kills what its old workers hold 240 s after every reload
  (`SCALE.md`, Pain Point 9). An `ssh-edge` rollout drains instead of cutting:
  the new pod binds beside the old one (SO_REUSEPORT) and the old one soft-stops
  through a `kill -USR1 1` preStop hook, keeping its sessions for up to the 6 h
  grace period; a `FailedPreStopHook` event when it exits is expected. Its
  `tcp-tuning` init container sets `net.ipv4.tcp_slow_start_after_idle=0` in the
  node's host network namespace.
- **IPv6 HTTP(S).** After the maintenance cutover, ingress-nginx uses
  `hostNetwork` and binds 80/443 directly on IPv4 and IPv6. It ignores
  client-supplied forwarding headers and sends its observed socket address to
  `late-ssh` for pairing limits. The former `kube-system/ipv6-proxy` lost the
  IPv6 client address; its removal and rollback are below. `Network is
  unreachable` on `ssh -6 late.sh` means the client lacks IPv6.
- **SSH pod drain.** `service-ssh` has `termination_grace_period_seconds =
  21600` (6 h): a rollout stops new connections and lets sessions drain before
  SIGKILL. A pair websocket that drops during that window cannot reconnect to
  the old pod (`SCALE.md`, Pain Point 10).

## Database

Production Postgres runs as a CloudNativePG cluster in Kubernetes.

Fastest working path for interactive inspection is `scripts/connect_db.sh`. It discovers the current pod behind the read-write service, port-forwards it through `kubectl`, reads generated CNPG credentials from the Kubernetes Secret at runtime, stores the password only in a temporary `.pgpass` file, and deletes that file when `pgcli` exits.

```bash
# Requires local kubectl access to the production cluster and local pgcli.
scripts/connect_db.sh

# Optional overrides:
KUBE_CONTEXT=prod KUBE_NAMESPACE=default scripts/connect_db.sh
LATE_DB_LOCAL_PORT=15433 scripts/connect_db.sh
```

Notes:

- Defaults follow Terraform: namespace `default`, service `postgres-rw`, secret `postgres-app`.
- Override the service, secret, or pod with `LATE_DB_KUBE_SERVICE` / `LATE_DB_KUBE_SECRET` / `LATE_DB_KUBE_POD` if infra names change.
- For ad hoc prod inspection, prefer read-only `SELECT` queries.
- The script intentionally never prints the database password or passes it in the `pgcli` command line.

## Incident triage

When prod looks down or slow, start with Kubernetes state, then metrics shape, then DB evidence. Do not assume "too many users" unless the active-session metrics support it. Before diagnosing from scratch, read the incident log below: past crashes with verdicts, shipped fixes, and what-to-check-next live there, and every new investigation should add an entry.

Useful first pass:

```bash
kubectl get pods -n default -o wide
kubectl top pods -n default
kubectl get events -n default --sort-by=.lastTimestamp
kubectl get cluster -n default postgres -o jsonpath='{.status.currentPrimary} {.status.targetPrimary} {.status.phase}{"\n"}'
kubectl get pod -n default <pod> -o jsonpath='{.status.containerStatuses[0].restartCount} {.status.containerStatuses[0].lastState.terminated.reason} {.status.containerStatuses[0].lastState.terminated.finishedAt} {.status.containerStatuses[0].state.running.startedAt}{"\n"}'
```

For a live Rust service memory check inside a pod:

```bash
kubectl exec -n default deploy/service-ssh -- /bin/sh -c 'cat /proc/1/status | egrep "VmRSS|VmSize|VmHWM|Threads"'
```

VictoriaMetrics is usually available in-cluster as `monitoring/victoriametrics` on `127.0.0.1:8428` from inside its pod. Query it from there when local ClusterIP access hangs:

```bash
kubectl exec -n monitoring deploy/victoriametrics -- wget -qO- 'http://127.0.0.1:8428/api/v1/query?query=max_over_time(container_memory_working_set_bytes{namespace="default",pod="<pod>",container="<container>"}[12h])'
```

Postgres has `pg_stat_statements` preloaded in prod. Use it to find heavy time, row-volume, and temp-spill fingerprints:

```sql
SELECT extname, extversion FROM pg_extension WHERE extname = 'pg_stat_statements';
SHOW shared_preload_libraries;

SELECT calls,
       round(total_exec_time::numeric, 1) AS total_ms,
       round(mean_exec_time::numeric, 2) AS mean_ms,
       rows,
       temp_blks_read,
       temp_blks_written,
       left(regexp_replace(query, '\s+', ' ', 'g'), 180) AS query
FROM pg_stat_statements
ORDER BY total_exec_time DESC
LIMIT 12;

SELECT calls,
       rows,
       round(rows::numeric / nullif(calls, 0), 1) AS rows_per_call,
       round(mean_exec_time::numeric, 2) AS mean_ms,
       left(regexp_replace(query, '\s+', ' ', 'g'), 180) AS query
FROM pg_stat_statements
WHERE calls > 0
ORDER BY rows DESC
LIMIT 12;

SELECT calls, temp_blks_read, temp_blks_written, round(total_exec_time::numeric, 1) AS total_ms,
       left(regexp_replace(query, '\s+', ' ', 'g'), 180) AS query
FROM pg_stat_statements
WHERE temp_blks_read > 0 OR temp_blks_written > 0
ORDER BY temp_blks_written + temp_blks_read DESC
LIMIT 12;
```

Read Postgres logs around an OOM/failover from the affected pod and prior container:

```bash
kubectl logs -n default <postgres-pod> -c postgres --previous --tail=300
kubectl logs -n default <postgres-pod> -c postgres --tail=300
```

## Chat feedback dumps

`scripts/dump_chat_room.sh` snapshots public topic rooms to text (`#bugs`,
`#suggestions` by default; `LATE_DUMP_DIR=feedback` refreshes the committed
copies). Every message header ends with `id=<uuid>`. Feed those ids to
`scripts/delete_chat_messages.sh` to prune handled entries: it deletes exactly
the ids given, nothing else, after a preview and a confirmation prompt.

## Debugging checklist

1. SSH won't connect → Check `open_access` and the limits in the active config.rs profile, SSH key path
2. No audio → Check Icecast container, Liquidsoap container, the `audio_base_url`/`icecast_url` profile values. If streams are down, verify fallback music exists on the PVC (see below)
3. Paired controls not reaching the client → Check the pair WS connection, token mismatch, SessionRegistry
4. Audio source not switching → Check pair WebSocket connectivity and the persisted user `audio_source`/stream settings
5. Chat not syncing → Check DB connectivity, 10s refresh cadence, snapshot/event channels
6. Now-playing shows "Unknown" → Check Icecast `/status-json.xsl`, metadata format: `"Artist - Title | Duration"` (duration is absent for internet streams; this is expected)
7. Liquidsoap debugging → `docker run --rm savonet/liquidsoap:v2.4.0 liquidsoap -h <topic>`
8. Music missing from PVC → Dispatch the `sync_music` workflow for the environment (syncs from R2; manual only, never part of an infra deploy). For manual recovery: `aws s3 sync s3://$MUSIC_BUCKET/ ./music/ --endpoint-url $S3_ENDPOINT` then `kubectl cp` each genre dir individually into the pod.
9. Repeated Postgres `role "root" does not exist` lines in GitHub Actions are often service-log noise, not the failure. They’re misleading because Actions prints service container logs after a job fails. Generally check for other errors before stopping to try and fix this probable red-herring.
10. `service-ssh`/Postgres OOM with low user count → work through the incident log (below): Lateania furniture amplification first, then the per-session pair/WS amplifiers from the 2026-07 OOMs.
11. NetHack door dead for everyone with `Too many hacks running now` → orphaned getlock slots; see the NetHack entry below and `late-ssh/src/app/door/nethack/DEPLOY.md`, Operations.

## Releases and deploys

Every published GitHub Release lands in `release.yml`, which parses the tag suffix once and routes to exactly one deploy workflow: `deploy_service.yml` for ssh (no suffix), web, bastion, and every door game; `deploy_cli.yml` for `-cli`; `deploy_infra.yml` for `-infra`; and for `-full`, both in sequence (`deploy_service.yml` with component ssh, then `deploy_infra.yml` gated on it) so one release ships an ssh image and a terraform apply in skew-rule order. Each of those also supports manual `workflow_dispatch` with explicit `release_tag` and `environment` inputs, the recovery path when GitHub misses a release event; shared `ci`, `build`, and `terraform` workflow calls accept `source_ref`, so manual deploys check out the requested tag instead of the UI-selected branch. `deploy_service.yml` is image-only for every component: `ci` + `build` (docker target `runtime-<component>`; the Dockerfile gives each door its own `builder-<door>` stage, so a door image build compiles only that host crate, never `late-ssh`/`late-web`), then `kubectl set image deployment/<name> '*=<image>'` and a rollout wait. It never runs terraform on ordinary releases, so nothing else in the cluster is touched. Doors self-bootstrap: a `check_deployment` job reads live cluster state, and when the door's deployment is missing (first deploy, disaster recovery) the run switches to `terraform.yml` with `targets: module.door["<game>"]`, scoping the apply to exactly that game's resources plus dependencies; no manual dispatch needed. Manifest changes ship through `deploy_infra.yml` (full apply). Terraform never needs current image tags: every deployment carries `ignore_changes` on its images, so applies leave running images alone, and the `IMAGE_TAGS` terraform var only matters when a deployment is being created. Door asset images (the pinned upstream binaries) build in `doors.yml`, one matrix over `docker/doors/`, publishing each `door-<game>` at the tag pinned in the root Dockerfile's `FROM` lines (the single source of truth). The deploy skew rule is below.

**Deploy skew rule.** A profile change that adds or removes a required secret
must ship its image before or together with the Terraform apply that changes
the injected env; an old image against a new pod template crash-loops on
`... must be set` (the 2026-07-31 incident below). Releases are image-only, so
the order is explicit: deploy the tolerant image (`vX.Y.Z` for ssh, `-web` for
web), then ship the pod-template change with an `-infra` release. A `-full`
release does both legs in that order for ssh without double-rolling it
(Terraform ignores the ssh image and injects secrets by `secret_key_ref`). A
change to `late-web/src/config.rs` needs a `-web` release; an ssh release never
applies infra drift. Prod env is `LATE_ENV=prod` plus secrets only, injected by
`infra/service-ssh.tf` / `infra/service-web.tf`.

## First bastion and IPv6 HTTP cutover

The repo owner approved an out-of-service window for this first switch. The
ordered preflight, port handoff, public-edge checks and rollback are in the
[bastion deployment guide](../devdocs/BASTION-DEPLOY-GUIDE.md).

## Monitoring gotchas

- Provisioned Grafana dashboards reference the metrics datasource by its stable
  UID `victoriametrics`, never a generated id.
- VictoriaLogs `stats` panels are numbers only: `by (...)` fields become labels
  and every other column is parsed as a float, so a string column fails the
  panel with `unable to parse log value to float64`. Group by a name
  (`stats by (user_id, username)`), never select it with an aggregate. To show a
  field in a `logs` panel, format it into `_msg`
  (`| format "[<username>] <_msg>" as _msg`).
- VictoriaTraces needs `--servicegraph.enableTask=true` for the service graph.
  Its Jaeger API is on `monitoring/victoriatraces:10428`.

## Incident log

One dated entry per production crash or serious incident. **Add a new entry (newest first) whenever a crash is investigated**, even if the root cause stays unproven; the negative evidence ("we checked X, it was clean") is exactly what the next investigation needs. Keep verdicts honest: "consistent and plausible, not proven" beats a confident wrong root cause. Triage commands and the symptom checklist are above; performance (non-crash) findings are in `SCALE.md`.

Entry template: symptom → evidence → verdict → fixes shipped → what to check if it repeats.

### 2026-10-12: cert renewal reloaded ingress-nginx, every SSH session dropped 240 s later (no crash)

- **Symptom:** at 18:50:39Z every SSH session and paired websocket disconnected at once and clients reconnected within ~20 s, which looked like a crash. `service-ssh` had 0 restarts (up since 09:42Z); nodes and every other pod were healthy.
- **Evidence:** cert-manager renewed `livekit-whip-tls` (`whip.late.sh`) over HTTP-01. Adding the solver ingress reloaded ingress-nginx at 18:46:39.08Z (`Backend successfully reloaded` in the controller log), and removing it reloaded again at 18:47:03Z. 18:46:39 + `worker_shutdown_timeout` 240 s = 18:50:39; late-ssh logged the first of 23 `websocket dirty close or error ... ResetWithoutClosingHandshake` at 18:50:39.26Z and 22 `registered cli session token` reconnects by 18:50:58Z. The stragglers around 18:51 are connections opened between the two reloads. Every IPv4 SSH transport peer was `10.42.0.31`, the controller pod, so port 22 was riding nginx; IPv6 peers were the node address via `ipv6-proxy`.
- **Verdict:** proven. The reload drop that SCALE.md Pain Point 9 described and had accepted.
- **Fix prepared:** public `22`/`6697` moved off ingress-nginx onto the `kube-system/ssh-edge` host-network HAProxy (`infra/ssh-tcp.tf`), which no Ingress or certificate change touches; its own rollouts surge and soft-stop instead of cutting (verified in Docker: the surge pod binds beside the old one, a session open across `SIGUSR1` completes, and the old process exits after it). The cutover apply rolls the ingress-nginx controller once, so public ports refuse connections while the old controller pod drains (up to ~5 min) and every session drops one last time.
- **If it repeats:** look at late-ssh `transport=` peers. A pod address like `10.42.0.31` means port 22 is back on nginx; the node address `46.62.210.86` means `ssh-edge` is serving it, so check `kubectl get pods -n kube-system -l app=ssh-edge` for age and restarts, and the HAProxy tcplog (`kubectl logs -n kube-system ds/ssh-edge`) for when sessions closed.

### 2026-09-03: SSH connection flood took the ingress down (edge outage ~5 min, all sessions dropped)

- **Symptom:** from 06:59Z a TCP connection flood hit port 22. At ~07:02:40Z the ingress-nginx controller failed its liveness probe (1 s timeout on `/healthz`), kubelet killed it, nginx drained for four minutes (listeners closed, every public port 22/80/443/6697 refused), restarted at ~07:07Z and dropped all 18 proxied SSH sessions. `service-ssh` itself never restarted. Locally the CLI reported "Network is unreachable" because the machine has no IPv6 route and the IPv4 attempt was the refused one; the CLI surfaces only the last error.
- **Evidence:** VictoriaLogs shows late-ssh going from ~10 lines/min to 8.8k (06:59), 25.6k (07:00), 41.7k (07:01). Messages: 47k `failed to resolve proxy protocol header; dropping connection`, 29.6k `connection limit reached, rejecting new client` (the 1000-permit global semaphore was exhausted from 06:59 on, so any real login in that window was rejected at auth), 2.3k `ssh rate limit exceeded`, 2k `per-ip limit reached`. `late_ssh_connections_total` peaked at 16.3k/min (those are only connections that got past the PROXY header). Phase 1 (06:59-07:03) was distributed: 361 distinct client IPs in the old nginx log, ~25 connections each, and 9,190 of them were still open (>200 s) when nginx drained at 07:06, i.e. they were held, not churned. Phase 2 (07:08Z onward, still running at 07:14Z, ~6k/min) is a single IP, `189.217.206.172` (39.5k rate-limit rejects in 8 minutes). Ingress container CPU went 0.04 to 4.9 cores, node root cgroup 1.7 to 7.2 of 8 cores at 07:03, so the controller's Go healthz endpoint starved behind nginx workers and the 1 s probe tripped. Ingress memory 149 to 696 MiB, late-ssh working set 700 to 1814 MiB from held connections. Kubelet PLEG p99 stayed <50 ms: no node stall, pure CPU contention. No prior minute in the last 7 days exceeded 300 connections/min.
- **Verdict:** proven. A port-22 DDoS, first a ~360-host botnet holding connections open, then one host churning. The ingress died from CPU starvation of its health endpoint, not from nginx limits (worker_connections 16384 x 8, nofile 1M, none hit; no nginx error lines besides upstream resets).
- **Weaknesses it exposed:** (1) over-limit connections in `ssh.rs` `new_client_with_addrs` are not dropped: they get a full SSH handshake and only fail at auth/channel-open, so a bot that never authenticates holds its global permit, its tokio task and russh buffers until `ssh_idle_timeout` (3600 s) or keepalive reaps it; that is how 1000 permits vanished in one minute and real users got rejected. (2) The global semaphore is acquired before the per-IP rate limiter, so rate-limited connections still burn permits. (3) The ingress liveness probe has a 1 s timeout and the controller has no CPU limit or priority, so any traffic spike that saturates the node converts into a 300 s drain outage of every port. (4) There is no edge firewall in `infra/` (no `hcloud_firewall`, no nginx per-IP limits on the TCP services), so the only per-IP defence is inside late-ssh, after nginx has already paid for the connection.
- **Fixes shipped (2026-09-03, same day):** (a) `ssh.rs`: admission moved ahead of the handshake. `Server::admit` runs the per-IP rate limiter, then the per-IP count, then takes the global permit last; any refusal returns before `run_stream`, so the socket closes on the spot and holds nothing. `over_limit` and the auth/channel-open rejections are gone; a refused client sees a TCP close, not an auth failure. New metric `late_ssh_connections_rejected_total{reason=proxy_header|rate_limited|per_ip_limit|global_limit}`; `late_ssh_connections_total` still counts every accepted socket, so rejected/accepted is the flood ratio. Tests in `ssh_test.rs`: rate-limited socket closed without a banner, rate-limited peer does not consume a global permit (distinct client IPs supplied by trusted PROXY v1 headers over localhost, without binding loopback aliases), global limit closes the socket and frees the slot on disconnect. (b) `infra/ssh-tcp.tf`: ingress liveness/readiness probes 5 s timeout x 6 failures (was 1 s x 5 / x 3), controller requests 250m/256Mi and a 3-core CPU limit so nginx cannot take the node. Not done on purpose: per-IP `limit_conn` in nginx stream and HAProxy stick tables (marginal once (a) closes sockets instantly; revisit if a flood ever makes HTTP slow), and any IP blocklist (Hetzner Cloud Firewall is allow-only, and phase 1 used 361 addresses).
- **If it repeats:** `late_ssh_connections_total` > 300/min is abnormal, and `late_ssh_connections_rejected_total` by `reason` says which gate is doing the work; `stats by (_msg) count()` in VictoriaLogs tells the phase (proxy-header timeouts = edge proxy overloaded, `connection limit reached` = permits exhausted, `rate limit exceeded` by `ip` = who). Port 22 sits on `ssh-edge`, not nginx: its HAProxy tcplog (`kubectl logs -n kube-system ds/ssh-edge`) gives client IPs and per-connection durations, and connections lasting >200 s in a flood mean held sockets. `ssh-edge` CPU pinned at its 1-core limit is the flood signature; it has no liveness probe, so it slows down instead of being killed.

### 2026-08-08: shared IRC ingress address captured by a server ban (IRC outage, no crash)

- **Symptom:** users reconnecting after a service restart received IRC `465 You are banned from this server`; regenerating IRC access tokens did not help.
- **Evidence:** affected users had unrelated public addresses, while the active server-ban list contained `10.42.0.123`, a Kubernetes-internal ingress address. A raw TLS registration exposed the `465` that clients summarized as generic authentication failure. IPv4 ingress-nginx and the IPv6 HAProxy path did not send PROXY metadata for IRC, and `ircd::serve` passed the TCP transport peer into IP-ban auth and active-session state. A username ban of an IRC-connected account therefore captured the shared ingress address; established connections survived until restart, then all reconnects matched it before token lookup.
- **Verdict:** proven. IRC confused its proxy transport peer with the client IP.
- **Fix prepared:** the poisoned live ban was replaced without an IP. IRC now parses trusted PROXY v1 metadata before rustls, carries the verified client IP as optional state, and never substitutes the transport address for ban matching or persistence. Terraform keeps parser acceptance separate from ingress emission: deploy the parser-capable image with acceptance enabled and emission disabled, then enable emission in a subsequent infrastructure apply. Optional no-header parsing protects only the new-parser/old-ingress direction; it does not protect an old parser after ingress starts emitting PROXY. Rollback disables emission before rolling back below the parser-capable image.
- **If it repeats:** run `/mod view bans server` and look for pod/node addresses, confirm the `ssh-edge` IRC backend uses `send-proxy` (`IRC_PROXY_EMIT`), and inspect startup config/logs for the profile's `irc.proxy_protocol`/trusted CIDRs, missing headers, or proxy-parse failures.

### 2026-08-06: news AI summaries dead after the 3.6-flash switch (feature outage, no crash)

- **Symptom:** every AI-summarized news link (articles and YouTube alike) fell back to generic summaries after the `model_update` commit (5829f9c6) moved `AI_MODEL` to `gemini-3.6-flash`. Logs: `gemini returned no usable text call=generate_json_with_search`, raw body holding only `usageMetadata` (thinking tokens spent, zero output). @bot and @bartender kept working.
- **Evidence:** reproduced live with curl. On 3.6-flash, combining the `googleSearch` tool with `responseMimeType: application/json` returns HTTP 200 with no `candidates` key at all; the identical request without the mime type answers normally. `generate_json_with_search` was the only caller of that combination, which is why the other three AI paths (grounded plain text, ungrounded schema JSON) were unaffected. Reverting was not an option: `gemini-3.1-pro` now 404s on v1beta.
- **Verdict:** proven. Grounding plus JSON response mode is unsupported on 3.6-flash and fails silently instead of erroring.
- **Still standing on 2026-08-30**, unverified: `AI_MODEL` moved to `gemini-3.7-flash` and nobody re-ran the curl. `generate_json_with_search` still asks for JSON through the prompt only, so nothing depends on the answer today; re-test before ever putting `responseMimeType` back on a grounded call. The budget half of this outage (thinking tokens billed as output and spent out of `maxOutputTokens`, leaving a body of nothing but `usageMetadata`) is a property of the Gemini 3 Flash family, not of 3.6, and applies unchanged.
- **Fixes shipped:** `generate_json_with_search` no longer sends `responseMimeType`; JSON is requested via the prompt only, and a `strip_json_fence` helper in `app/ai/svc.rs` unwraps the ```json fence the model adds despite instructions. Callers already tolerated parse failures.
- **If it repeats:** a 200 whose body has no `candidates` key (only `usageMetadata` with `thoughtsTokenCount`) means the model rejected the request shape, not the content; diff the failing path's `GeminiRequest` against the working ones (tools versus response mime/schema) before suspecting prompts or quotas. `first_text` logs the raw body at WARN.

### 2026-07-31: service-ssh stuck rollout + tainted codekeep PVC (deploy pipeline wedge, no user impact)

- **Symptom:** every terraform apply failed with `prevent_destroy` on `kubernetes_persistent_volume_claim_v1.codekeep_save` (tainted, plan wants replace), blocking all releases. Separately, a spare `service-ssh` pod (RS `6455988bf7`) crash-looped for a day (281 restarts) while the old pod kept serving; no user-facing outage, sessions never dropped.
- **Evidence:** crash log of the looping pod: `failed to load configuration: LATE_AI_MODEL must be set`. The `model_update` commit (5829f9c6, 2026-07-30) removed `LATE_AI_MODEL` from both code and `infra/service-ssh.tf`, but the cluster still ran image v0.42.4 (pre-removal, still requires the var). The 2026-07-31 codekeep deploy applied the new env-less pod template while keeping v0.42.4 (its `get_images` job reads the live tag), so the replacement pod could never boot. The same apply created the codekeep PVC with `wait_until_bound = true`; `local-path` is WaitForFirstConsumer and the consumer deployment had not been created yet, so the bind wait timed out, terraform marked the PVC tainted, and the apply died before creating `late-codekeep`/its Service. The PVC never bound and held no data.
- **Verdict:** proven. Two independent wedges from one apply: config/image skew (env removed from template before an image that tolerates its absence was deployed) and a bind-wait deadlock (PVC bind waits on a consumer the same apply had not created yet).
- **Fixes shipped:** `wait_until_bound = false` on the codekeep PVC (already in tree before this investigation); game deploy workflows rewritten to image-only `kubectl set image` so a game release can no longer touch `service-ssh` or any other cluster resource. Recovery: untaint the PVC (`terraform untaint kubernetes_persistent_volume_claim_v1.codekeep_save`, needs R2 state credentials), publish a `-codekeep` release, the workflow auto-bootstraps when the deployment is missing (targeted apply, touches only codekeep resources), then deploy a post-`model_update` ssh image (v0.42.7+) to heal the rollout.
- **If it repeats:** a crash-looping pod next to a healthy one = stuck rollout, read `kubectl logs --previous` of the looping pod first; a config-load error means template/image skew, and the fix is deploying an image built from the same ref as the template, not touching the healthy pod. When removing a required env var, deploy the image that stops requiring it before or together with the template change. New PVCs on `local-path` must use `wait_until_bound = false` or be created in the same apply order as their consumer.

### 2026-07-22: service-ssh OOM during Hacker News traffic surge

- **Symptom:** at 06:36:06Z the single `service-ssh` container was OOM-killed (exit 137) at its 4 GiB memory limit, disconnecting all active sessions. Kubernetes restarted it immediately; Postgres stayed healthy and did not fail over. The recovered service was healthy with 33 sessions, ~677 MiB working set, and no second restart during the investigation.
- **Evidence:** active sessions were 61 immediately before the kill (64 peak in the preceding minutes), so this was not connection-limit exhaustion. Working set was ~1.51 GiB at 06:36:00, jumped to ~2.32 GiB at the 06:36:05 scrape, then crossed 4 GiB and died before the next scrape. Two earlier transient spikes recovered without a restart: ~3.80 GiB at 05:27 and ~2.77 GiB at 06:25. Previous-container logs contain no `clamped oversized` resize, unsolicited clipboard, panic, input-queue saturation, or DB-pool timeout marker; terminal dimensions near the kill were normal and within the 500×200 clamp. The repeated `idx_users_username_lower` profile-edit error from one session was noisy (103 failures in a sampled ten minutes) but ended well before the kill; Postgres remained lightly loaded with no blocked or long-running query at investigation time.
- **Root cause (follow-up investigation, same day):** stalled-SSH-client output buffering in russh. Every rendered frame goes through `handle.data()` with a 50 ms timeout (`late-ssh/src/ssh.rs:1364`), but that timeout only measures getting a slot in russh's 128-entry per-session event queue. When a client stops reading (phone lock, flaky network, half-dead TCP), its SSH channel window hits zero and russh appends every subsequent frame to the per-channel `pending_data` VecDeque (russh 0.61.2 `lib_inner.rs`), which is **uncapped, non-blocking, and invisible** to the send timeout: no frame drop is counted, nothing is logged, and the session keeps rendering at 15-66 fps into the queue. `force_full_repaint()` after any dropped frame upgrades subsequent frames to full repaints (up to MBs each on large terminals), multiplying the rate. Memory frees all at once when keepalive (30 s x 3) or TCP reset reaps the connection.
- **Supporting evidence:** 5-second-resolution working set shows every spike is a step-up / hold 20-70 s / free-as-one-unit shape (05:27: +2.6 GiB freed in two ~1.45 GiB steps; 06:25: +1.41 GiB; 07:00 next morning: +1.52 GiB), matching per-connection buffers reaped by keepalive; the recurring ~1.4-1.5 GiB quantum = frame rate x frame size x fixed keepalive reap window, and OOM #2's +1.43 GB step was the same signature. `late_ssh_render_frame_drops_total` ran at exactly 909/min (one session, every world tick) 06:00-06:34 and 1818/min (two sessions) 06:22-06:30 overlapping the 06:25 spike: those are the *visibly* stalled sessions; the window-exhausted ones queue silently. Traces disproved the chat theories: `load_room_tail` ran at background rate (1-7/s) through both spikes, and prod room snapshots are KiB-sized, so the 06:35:58 `build_chat_snapshot` latency wave (20-50 ms to 388-705 ms) was starvation downstream of the memory pressure, not the allocator.
- **Fixes shipped:** ops first: memory limit raised 4Gi to 8Gi (+request 512Mi to 2Gi) via in-place pod resize (no restart; Kubernetes 1.34 `--subresource resize`) and persisted in `infra/service-ssh.tf`; headroom only, not the root-cause fix. Code, same day: output-budget guard in `late-ssh/src/ssh.rs`: an `OutputBudget` per session counts bytes handed to russh and resets when `Handler::window_adjusted` reports a positive window (russh flushes pending before the callback, so a positive window proves the backlog drained); over 32 MB outstanding the render loop pauses entirely (ratatui diff state untouched, so resume needs no repaint), and 30 s of sustained stall disconnects the session instead of waiting for keepalive. New metrics `late_ssh_render_stall_{skips,disconnects}_total`. The mpsc-timeout frame-drop path keeps its `force_full_repaint` (still correct there) but no longer re-arms the input-cadence retry: the repaint rides the next world tick, capping the drop-retry loop at 15 fps instead of 66. Pair-WS hardening in the same pass: per-token registration cap (8), per-IP concurrent pair-socket cap (reuses `max_conns_per_ip`, separate counter from SSH), and the per-connection control channel is now bounded (64) with try_send drop-on-full, mirroring the web tunnel.
- **Still open from the audit** (real, not this incident's cause): `ChatEvent::RoomTailLoaded` broadcasts ~500-message payloads to all sessions with a lag-triggered reload loop; Clubhouse `draw_tavern` heap-allocates one `String` per visible cell per frame; session bootstrap fan-out is unthrottled and deep-clones the nonogram library per session.
- **If it repeats:** the giveaway is the step / hold / free-as-unit shape at 5 s resolution with the ~1.4 GiB quantum. Correlate with `late_ssh_render_frame_drops_total` (a flat 909/min-per-session run = a visibly stalled client; silent window-exhausted clients show nothing), then identify the stalled session via keepalive disconnect logs at the free moment. VictoriaTraces (Jaeger API on `monitoring/victoriatraces:10428`) has per-operation spans for ruling out DB/chat paths quickly.

### 2026-07-12/13: service-ssh OOM #2 (2 GB step, plateau, then kill)

- **Symptom:** RSS jumped 630 MB → 2056 MB between 12:12:30 and 12:13:00, sat flat at ~2 GB for two full minutes, then OOM-killed at 12:15:18. Working set was still ~2.2 GB at the last 30s scrape, so a second spike past the limit happened inside the final 18s. The plateau-then-second-spike shape is NOT what a single bogus resize looks like (that is one instant spike, then freed or immediate OOM).
- **Evidence:** every terminal size found in traces was normal (77×37, 86×48, 173×48); no oversized pty or window-change was captured. The terminal-size theory holds on magnitude (ratatui-core 0.1.0 `Rect::new` does not clamp area, `Buffer::filled` allocates `width*height` Cells (~32 B each) and `Terminal` holds two buffers, so a hostile ~8000×4000 resize ≈ 2 GB) but it was never proven to be this incident's cause.
- **Verdict:** unproven. The resize clamp (`late-ssh/src/terminal_size.rs`) is correct defensive hygiene either way.
- **Fixes shipped (2026-07-13):** clipboard-image amplifier closed: the pair WS used to accept unsolicited `clipboard_image` payloads (up to 10 MB decoded each) into the per-session `channel(64)`, i.e. up to ~640 MB per session token; inbound clipboard payloads are now request-gated (`PairedClientRegistry::take_clipboard_request`, see `late-ssh/src/app/chat/CONTEXT.md`). Found in the same audit: RSS fetcher SSRF fixed (feed fetches now use the guarded downloader with per-hop redirect re-validation; `rss_feeds` contained live cloud-metadata probes), and `BrowserPaired` routing reverted to best-effort so a 250 ms send timeout no longer tears down the pair socket.
- **If it repeats:** run the triage above, then grep the previous container logs around the spike for clamp/pair markers and the new clipboard gate:

```bash
kubectl logs -n default deploy/service-ssh --previous --since-time=<spike-start> | rg 'clamped oversized pty dimensions|clamped oversized window resize|clamped oversized web tunnel resize|dropping unsolicited clipboard image|ws pair message routing timed out|ws pair request received|registered paired client|registered cli session token'
```

No clamp/clipboard hits → inspect other per-session payloads on `/api/ws/pair` (frame sizes, queued `SessionMessage` variants) and per-session allocations in the render path.

### 2026-07-12: service-ssh OOM #1 (single macOS pair correlation)

- **Symptom:** memory jumped ~800 MiB → ~2.3 GiB in one scrape while sessions only moved 13 → 14, correlated with one macOS native pair.
- **Fixes shipped:** client-controlled terminal dimensions clamped (500×200) in `late-ssh/src/terminal_size.rs`; 16 MB pair-WS frame cap.
- **Verdict:** terminal-size cause plausible, not proven (see OOM #2 above, which re-examined the data).

### 2026-05-19: audio queue stuck in a singleton violation (feature outage ~1h45m, no crash)

- **Symptom:** every booth submit failed with `duplicate key value violates unique constraint "idx_media_queue_single_playing"`; nobody could add tracks or vote-skip. One row sat at `status='playing'`.
- **Evidence:** after a pod restart `resume_from_db` adopted the playing row and its timer finished it normally. Later `finish_item` ran for a row no longer `playing`: `mark_played` matched 0 rows and `finish_item` returned early without clearing `state.current_item_id`. Staff `/audio skip` then read the stale id and `update_status` (no expected-status filter) rewrote a `played` row to `skipped`, cleared memory and advanced. From then memory held no current while the DB had a `playing` row, so every `mark_playing` hit the singleton index.
- **Verdict:** proven from logs. Non-atomic two-write transitions (DB status plus in-memory current id) diverged with no resync.
- **Fixes shipped:** the reconciliation contract in `late-ssh/src/app/audio/QUEUE.md` (`mark_skipped` with a status filter, `changed == 0` treated as drift, advance adopts the DB current first, a singleton violation reconciles), pinned by `audio/svc_test.rs`.
- **If it repeats:** look for a queue transition that bypasses the contract (a raw `update_status`, a `mark_*` whose zero-row result is treated as success). A pod restart clears the stale memory.

### Undated: service-ssh/Postgres OOM via Lateania furniture amplification

- **Symptom:** OOM with low user count; `mud_characters.data->'house_furniture'` inflated to huge arrays.
- **Check:**

```sql
SELECT u.username,
       pg_size_pretty(pg_column_size(m.data)::bigint) AS size,
       jsonb_array_length(coalesce(m.data->'house_furniture', '[]'::jsonb)) AS house_furniture
FROM mud_characters m
JOIN users u ON u.id = m.user_id
ORDER BY jsonb_array_length(coalesce(m.data->'house_furniture', '[]'::jsonb)) DESC
LIMIT 10;
```

- **Emergency cleanup** (if the field is inflated and cosmetic furniture loss is acceptable):

```sql
UPDATE mud_characters
SET data = jsonb_set(data, '{house_furniture}', '[]'::jsonb, true),
    updated = current_timestamp
WHERE jsonb_array_length(coalesce(data->'house_furniture', '[]'::jsonb)) > 0;
```

- **Fix shipped:** `late-ssh/src/app/door/lateania/svc.rs` replaces/dedupes/caps furniture on hydrate/save; without it users can re-amplify the field.

### Undated: NetHack door dead prod-wide (getlock slot exhaustion)

- **Symptom:** every launch fails with `Too many hacks running now`; the door is dead for all users.
- **Cause:** SIGKILL-while-live orphans a getlock slot; once all `MAXPLAYERS` slots are orphaned, `getlock()` fails for everyone.
- **Emergency fix:** `rm -f $VAR_PLAYGROUND/?lock.*` in the late-nethack pod (safe only while no games are live).
- **Fixes shipped:** SIGHUP hangup-save teardown before any SIGKILL, plus a boot-time orphan-lock sweep. Details in `late-ssh/src/app/door/nethack/DEPLOY.md` (Operations) and the invariants in its `CONTEXT.md` §7.
