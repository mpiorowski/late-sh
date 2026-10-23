# Bastion deployment guide

This is the checklist for the **first production bastion and IPv6 HTTP
cutover**. The repo owner approved a full out-of-service maintenance window:
existing direct SSH and IRC sessions are disconnected once. HTTP(S)
connections and audio streams are also interrupted during the handoff. Later
`late-ssh` replacements use the bastion reconnect path described in the
[feature doc](BASTION-FEATURE.md). Local evidence and its limits are recorded
in [BASTION-QA.md](BASTION-QA.md); no live cutover is claimed there.

The first `-infra` release switches public `:22`, changes ingress-nginx to
direct dual-stack host networking, and forgets the old IPv6 proxy from
Terraform state. **Do not publish it before traffic is blocked and the old
proxy's IPv6 ports are released.** The Terraform `removed` blocks do not
delete the live proxy.

## Preparation before the window

- [ ] **Manual — schedule and communicate the outage.** Agree on the test
  clients, observation period and rollback decision point. Keep an out-of-band
  or allowlisted management route: public SSH may be the operator's own path.
- [ ] **Manual — prepare release inputs.** Identify reviewed, matching ssh,
  infra and bastion refs/tags and confirm the production GitHub environment
  has `BASTION_HOST_KEY` and `BASTION_SHARED_SECRET`. Keep
  `BASTION_ENABLED=0` until the window. Record the current `IRC_PROXY_EMIT`
  value; the cutover needs `IRC_PROXY_EMIT=1` for IRC client-IP moderation.
  Choose the bastion host key deliberately: reusing the current public :22
  `SSH_HOST_KEY` preserves the fingerprint clients already trust. If rotating
  it, communicate the verified new fingerprint and plan client `known_hosts`
  updates. The compatible ssh image must deploy before the infra change that
  injects its tunnel secret; it can tolerate an
  absent or blank secret until then. If deploying that image early would
  disturb direct sessions, do that image release after blocking traffic.
- [ ] **Manual — save rollback inputs securely.** Capture the live
  `kube-system/rke2-ingress-nginx` HelmChart and HelmChartConfig, Terraform
  state, current working infra release/ref, and the values of
  `BASTION_ENABLED` and `IRC_PROXY_EMIT`. Record the current IPv4/IPv6
  listeners, public SSH host-key fingerprint, and a known-good direct SSH,
  IRC and HTTPS check.
- [ ] **Manual — review the actual production plan and node.** Confirm the
  Terraform plan has no unrelated destroys; the `ipv6_proxy` DaemonSet and
  ConfigMap are forgotten with `destroy=false`; ingress changes to
  `hostNetwork=true` with hostPort disabled; and `ssh-edge` routes `:22` to
  the bastion Service when enabled. Inspect the installed RKE2 ingress chart
  and its other host-network listener ports, including health and admission,
  for conflicts. Confirm the node owns the public IPv4 and IPv6 addresses and
  that only incumbent ingress/proxy processes occupy 80/443.
- [ ] **Manual — prepare isolated test accounts and clients.** Have two
  distinct external IPv6 addresses and an IPv4 address available. Record the
  address each test connection should show; keep a safe way to test bans and
  kicks without affecting other users.

## Cutover during the blocked window

1. [ ] **Manual — block new public SSH, IRC and HTTP(S)** at the external
   firewall. Preserve the management route and selectively allow test clients.
   Verify the block before changing listeners.
2. [ ] **Release — deploy the compatible `late-ssh` image**, if it was not
   already shipped, and wait for rollout. **Manual — terminate old direct
   sessions:** force-delete `service-ssh` pods rather than waiting for their
   six-hour drain; the Deployment will replace them. Confirm the replacement
   backend is ready. Example: `kubectl -n default delete pod -l
   app=service-ssh --grace-period=0 --force`. This ends IRC sessions too.
3. [ ] **Manual — free IPv6 HTTP ports:** delete the
   `kube-system/ipv6-proxy` DaemonSet, wait for its pod to disappear and for
   IPv6 80/443 to become unbound, then delete its ConfigMap. Use
   `kubectl -n kube-system delete daemonset ipv6-proxy`, verify the pod and
   listeners are gone, then run
   `kubectl -n kube-system delete configmap ipv6-proxy`. The upcoming
   Terraform apply only forgets these resources from state.
4. [ ] **Manual settings + release — set production `BASTION_ENABLED=1`
   and `IRC_PROXY_EMIT=1`, then publish the reviewed `-infra` release.** The
   compatible, parser-capable backend image must already be running. Wait for
   the apply and controller rollouts. It creates the bastion Service before
   `ssh-edge` resolves it,
   routes public SSH to that Service, injects the shared secret into backend
   pods, and moves ingress onto direct IPv4/IPv6 host networking. The new
   bastion Deployment can have a placeholder image and need not be ready yet.
5. [ ] **Release — publish the matching `-bastion` release** and wait for its
   Deployment rollout. Confirm the bastion host key and secret mounts, the
   backend's private `/tunnel` listener, `ssh-edge` PROXY v1 forwarding to
   bastion and IRC, and the relevant NetworkPolicy. Verify the public SSH
   host-key fingerprint against the planned value. Inspect the rendered
   ingress pod:
   host networking on, hostPort off, actual IPv4/IPv6 80/443 listeners, old
   proxy absent, and no trust of client-supplied forwarding headers.
6. [ ] **Manual — run the public-edge checks** in the table below from
   allowlisted external clients. Check actual backend/session IP attribution,
   limits, moderation and TLS behavior. Resolve failures before reopening.
7. [ ] **Manual — replace a backend while a test SSH/CLI session is live.**
   Verify SSH continuity, fresh TUI, paired CLI recovery after the live
   session returns, and cleanup of the retired generation's routes and counts.
   Exercise an explicit `r` reconnect and an abnormal backend loss. Confirm
   terminal clients close cleanly on quit/kick/ban.
8. [ ] **Manual — reopen public traffic** only after all checks pass. Watch
   connection, readiness, error and rate-limit signals through the agreed
   observation period. Keep `BASTION_ENABLED=1` in the production environment
   for every later infra release; omission defaults to `0` and would send
   new SSH connections directly to the backend.

## Public-edge acceptance checks

| Path | Verify from real clients |
| --- | --- |
| SSH via bastion, IPv4 and IPv6 | Correct key/account and host key, stable SSH connection over backend replacement, expected original client IP in backend admission/session data and ban snapshot; bastion/edge pod IP appears only as transport peer. A ban of one test IP must leave an unrelated account on a different IP able to connect. Kick ends only the target sessions. |
| TLS IRC, IPv4 and IPv6 | Correct real client IP for moderation when PROXY metadata is present. Missing/unknown metadata must not turn the shared edge address into a ban. |
| HTTP and HTTPS, IPv4 and IPv6 | HTTP-to-HTTPS redirect, TLS/certificates, `late.sh` and API routes, cert-manager HTTP-01 routing, and direct node listeners. A deliberately false external `X-Forwarded-For` must not become the resolved API/pairing address. |
| Pair WebSockets | A CLI token pairs to its live SSH session; two distinct IPv6 clients retain independent attempt/socket limits; replacement and reconnect restore pairing. The HTTP client's IP does not overwrite the SSH moderation IP. |

These are live RKE2, HAProxy, CNI, Secret and ingress checks. The local
[qualification campaign](BASTION-QA.md) and disposable Kind ingress check do
not substitute for them. The separate [open liveness item](BASTION-FEATURE.md#open-work)
requires a silent-network-blackhole cleanup test before public cutover.

## Rollback while traffic stays blocked

1. [ ] **Manual — keep the firewall block** and set production
   `BASTION_ENABLED=0`; restore the saved `IRC_PROXY_EMIT` value before
   applying the old infra ref. Preserve the failed-run logs, plan and live
   listener state for diagnosis.
2. [ ] **Manual — restore the saved pre-cutover HelmChartConfig** so
   ingress returns to its former hostPort layout. Wait for host-network
   ingress listeners to exit and IPv4 80/443 to answer. Do not bring the
   IPv6 proxy back while ingress still owns the same IPv6 ports.
3. [ ] **Release/manual — apply the prior approved infra ref with
   `BASTION_ENABLED=0`.** The old configuration recreates the IPv6 proxy
   DaemonSet and ConfigMap, which the new configuration forgot from state.
   Wait for IPv6 80/443 and direct SSH/IRC to become healthy.
4. [ ] **Manual — repeat the client-IP, TLS, SSH/IRC and HTTP checks** on the
   restored paths, then reopen traffic. If an apply fails partway, inspect
   live listeners and the Terraform plan before retrying; never run the old
   proxy and host-network ingress on the same IPv6 ports.

## Release and later-upgrade notes

Published `vX.Y.Z` tags route to the ssh image deploy; `-infra` applies
Terraform; `-bastion` builds and rolls the bastion image. The service deploy
sets the Deployment image and waits; it does not bootstrap a missing bastion
Deployment. The infra apply creates that Deployment without waiting for its
placeholder image; the following `-bastion` release installs the real image.
The workflows also support manual dispatch with an explicit release tag and
environment. See [infra/README.md](../infra/README.md) and the general
[runbook](../infra/RUNBOOK.md) for release and deployment mechanics.

After cutover, normal `late-ssh` image releases can replace backends while
the bastion holds SSH transports. Bastion image releases still disconnect
all active bastion sessions. A future infra apply must retain the production
flag and any required secrets. Re-run the public-edge IP and pairing checks
after changing ingress, `ssh-edge`, trusted peer ranges or proxy settings.
