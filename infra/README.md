# late.sh Infra

## Infrastructure Setup

Follow these steps to provision the infrastructure for late.sh.

### Prerequisites

You need at least one Linux server (VPS or bare metal) with:

- **OS:** Debian 12+, Ubuntu 22.04+, RHEL 9+, or any
  [RKE2-supported distro](https://docs.rke2.io/install/requirements#operating-systems)
- **Arch:** x86_64 or aarch64
- **CPU:** 4 vCPUs
- **RAM:** 8 GB
- **Disk:** 40 GB+
- **SSH access** with a key pair

Providers like Hetzner, DigitalOcean, or AWS EC2 all work. For HA, provision 2-3
server nodes.

### 1. Set Up Local Environment

```bash
cp .env.example .env
```

Edit `infra/.env` with your server details (IP, SSH user, key path, name).

### 2. Set Up Kubernetes Cluster (RKE2)

```bash
sh setup_rke2.sh
```

Installs RKE2, configures kubeconfig, and creates the `staging` GitHub
environment.

### 3. Configure Application

```bash
gh auth login -s write:packages
sh setup_app.sh
```

You'll be prompted for:

- **Domain** (default: `late.sh`)
- **S3-compatible storage** — endpoint, access key, secret key for TF state and
  DB backups
- **AI config** (optional) — Gemini API key for ghost chat features
- **Ghost users** (optional) — enable simulated presence

Auto-generated: SSH host key (Ed25519), Docker registry config.

### 4. Set Up DNS

Configure DNS A records pointing to your server:

```
late.sh      → <server-ip>
*.late.sh    → <server-ip>
```

For IPv6, configure matching AAAA records to the node IPv6 address. The
Terraform-managed `ipv6-proxy` DaemonSet binds only that IPv6 address and
forwards HTTP(S) into the existing IPv4 ingress path; SSH and IRC bind both
address families on `ssh-edge` (see below).

This enables:

- `ssh late.sh` — SSH TUI
- `irc.late.sh:6697` — IRC over TLS
- `https://late.sh` — Web landing + audio pairing
- `https://api.late.sh` — SSH API / WebSocket
- `https://audio.late.sh` — Icecast audio stream
- `https://rtc.late.sh` — LiveKit voice signaling
- `https://files.late.sh` — Public uploaded chat files (R2 custom domain)
- `https://grafana.late.sh` — Monitoring

`rtc.<domain>` must be reachable directly for LiveKit media ports. Do not use
standard Cloudflare proxying for this host unless the selected Cloudflare
product also forwards the raw WebRTC/TURN ports listed below; the browser/CLI
signaling path uses HTTPS/WSS, but media uses ICE/TCP, ICE/UDP, and TURN.

### 5. Set Up S3 Buckets

Create the required buckets in your S3-compatible provider:

- `{context}-tf-state` — Terraform state
- `{context}-db-backups` — Database backups

Optionally create a files bucket for public chat uploads:

- `{context}-files` — Public uploaded chat files

For Cloudflare R2, attach a custom domain such as `files.<domain>` to the files
bucket and set `FILES_PUBLIC_BASE_URL` to that exact public base URL.

### 6. Deploy

Create a release to trigger CI/CD:

```bash
# Staging
gh release create v0.1.0-rc --prerelease --title "Staging" --notes "Initial deployment"

# Production
gh release create v1.0.0 --title "Production" --notes "Initial deployment"
```

After the monitoring stack is deployed, retrieve the generated Grafana admin
password:

```bash
kubectl get secret -n monitoring grafana-admin -o jsonpath='{.data.password}' | base64 -d; echo
```

Login with:

- username: `admin`
- password: output of the command above

### 7. Upload Music

After first deploy, copy music files to the Liquidsoap PVC:

```bash
POD=$(kubectl get pod -n default -l app=liquidsoap -o jsonpath='{.items[0].metadata.name}')
kubectl cp -n default ./music/. "$POD":/music/ -c liquidsoap
```

## Architecture

| Component     | Service                                                                         | Ports                                                  | Description                                       |
| ------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------ | ------------------------------------------------- |
| late-ssh      | `service-ssh-sv`                                                                | 2222 (SSH), 4000 (API), 6697 (IRC TLS when enabled)    | SSH TUI server + HTTP API + embedded IRC          |
| late-web      | `service-web-sv`                                                                | 3000                                                   | Web landing page + pairing                        |
| Icecast       | `icecast-sv`                                                                    | 8000                                                   | Audio streaming server                            |
| Liquidsoap    | none (dials out to `icecast-sv`)                                                | -                                                      | Playlist encoder                                  |
| LiveKit       | `livekit-sv`                                                                    | 7880 (WSS/API), 7881 TCP, 7882 UDP, 3478 UDP, 5349 TCP | Voice-room SFU, ICE/TURN media                    |
| Minecraft     | none (hostPort)                                                                 | 25565 TCP, 24454 UDP (voice) on `agent-1`              | Paper server with GriefPrevention, whitelist-only |
| Vintage Story | none (hostPort)                                                                 | 42420 TCP+UDP on `agent-2`                             | Official server (arm64), whitelist-only           |
| PostgreSQL    | `postgres-rw`                                                                   | 5432                                                   | CloudNativePG cluster                             |
| Monitoring    | OpenTelemetry Collector, VictoriaMetrics, VictoriaLogs, VictoriaTraces, Grafana | various                                                | Full observability stack                          |

SSH on port 22 and IRC on port 6697, IPv4 and IPv6, reach the late-ssh pod
through `ssh-edge`, a host-network HAProxy DaemonSet (`ssh-tcp.tf`), never
through NGINX: every NGINX reload (each cert-manager renewal is one) kills the
connections its old workers hold 240 s later. HAProxy sends PROXY v1 metadata
(for IRC, only when `IRC_PROXY_EMIT` is on) before the application's in-process
TLS handshake; late-ssh accepts it only from the trusted CIDRs in its prod
profile (`late-ssh/src/config.rs`). LiveKit signaling is routed
through NGINX ingress on `rtc.<domain>`, while LiveKit media ports are bound
directly on the node by the `livekit` pod. On a fresh cluster, the `livekit` pod
may wait for cert-manager to create the `livekit-tls` secret used by embedded
TURN/TLS. If it sits in `ContainerCreating`, check certificate issuance before
treating the rollout as failed.

### Nodes

| Node       | RKE2 role                    | Placement                                   | Runs                                                                                                                                                   |
| ---------- | ---------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `server-1` | server (control plane, etcd) | untainted                                   | service-ssh, redis, doors, ingress-nginx, ipv6-proxy, ssh-edge, LiveKit, Postgres                                                                      |
| `agent-1`  | agent                        | label and `NoSchedule` taint `role=support` | service-web, Icecast, Liquidsoap, otel-collector, vmagent, kube-state-metrics, VictoriaMetrics/Logs/Traces, Grafana, Minecraft, second CoreDNS replica |
| `agent-2`  | agent (arm64)                | label and `NoSchedule` taint `role=games`   | Vintage Story                                                                                                                                          |

A workload lands on `agent-1` only when it carries both the `role=support` node
selector and toleration (`support_node_*` locals in `defaults.tf`), and on
`agent-2` only with `role=games` (`games_node_*`). The taints keep everything
else, service-ssh included, on `server-1`. `agent-2` is a Hetzner CAX21, arm64,
while every image we build for the other nodes is amd64 only, so a workload
headed there also selects `kubernetes.io/arch=arm64` and needs an arm64 image.
Of the cluster's DaemonSets only Canal (multi-arch, tolerates every taint) and
kube-proxy run there. Workloads with a
`local-path` volume stay pinned to the node that holds the data, so moving one
needs a fresh volume on the new node, not just the selector.

`agent-1` is a worker, not a second control plane: with two etcd members both
must be up, so a second server adds a failure mode without adding safety.
Control-plane HA starts at three servers.

#### Adding an agent node

`setup_rke2.sh` bootstraps a whole cluster, including reinstalling `server-1`
and overwriting kubeconfig and the `KUBE_CONFIG` secret, so do not rerun it to
add a node. On a fresh Debian host, as root:

```bash
# on your laptop: the join token
ssh -p 22222 root@<server-1-ip> cat /var/lib/rancher/rke2/server/node-token

# on the new host: admin SSH on 22222, like every node
tee /etc/ssh/sshd_config.d/99-admin.conf >/dev/null <<'CONFIG'
Port 22222
CONFIG
sshd -t && systemctl reload ssh

# on the new host: join as an agent at server-1's exact RKE2 version
mkdir -p /etc/rancher/rke2
tee /etc/rancher/rke2/config.yaml >/dev/null <<'CONFIG'
server: https://<server-1-ip>:9345
token: <token>
node-name: <agent-N>
node-label:
  - role=<support|games>
node-taint:
  - role=<support|games>:NoSchedule
CONFIG
chmod 600 /etc/rancher/rke2/config.yaml
curl -sfL https://get.rke2.io | INSTALL_RKE2_TYPE=agent INSTALL_RKE2_VERSION=v1.34.4+rke2r1 sh -
systemctl enable rke2-agent.service
systemctl start rke2-agent.service
```

Pin `INSTALL_RKE2_VERSION` to whatever `kubectl get nodes` shows for `server-1`:
an agent newer than its server is unsupported. The installer picks the host's
architecture itself, arm64 included. Nodes talk on TCP 9345 and 6443
(agent to server), UDP 8472 (Canal VXLAN) and TCP 10250 (kubelet) both ways;
open those in any Hetzner Cloud Firewall attached to either host.

### Minecraft

`infra/minecraft.tf` runs a Paper server on `agent-1`'s port 25565 as a pod
hostPort, deliberately not through the ingress-nginx TCP map: nginx reloads drop
long-lived TCP sessions, which would kick every player on each cert-manager
renewal. `late.sh` and the `*.late.sh` wildcard point at `server-1`, so two DNS
records, managed by hand like the rest of the zone, send players to `agent-1`:

```
mc.late.sh                A    <agent-1-ip>
_minecraft._tcp.late.sh   SRV  0 5 25565 mc.late.sh.
```

The explicit A record overrides the wildcard, and the SRV record lets the client
accept plain `late.sh`. Both must be DNS-only, never proxied: the traffic is raw
TCP. Simple Voice Chat adds a second hostPort, 24454 UDP, reached at the same
address, so it needs no DNS of its own; open it in any Hetzner Cloud Firewall
on `agent-1`. The world lives on the `minecraft-data` PVC (`local-path`,
`prevent_destroy`) on `agent-1`'s disk; `worldborder set 10400` at startup caps
its disk growth there.

Access is online-mode plus an enforced whitelist. `MINECRAFT_WHITELIST` and
`MINECRAFT_OPS` seed the lists on every boot. Day-to-day changes go through rcon
inside the pod and persist across restarts alongside the seeded names:

```bash
kubectl exec -n default deploy/minecraft -- rcon-cli whitelist add <name>
kubectl exec -n default deploy/minecraft -- rcon-cli whitelist remove <name>
kubectl exec -n default deploy/minecraft -- rcon-cli op <name>
kubectl exec -n default deploy/minecraft -- rcon-cli list
kubectl logs -n default deploy/minecraft --tail=200
```

Bumping the game version (`minecraft_version` in `defaults.tf`) is a one-way
world upgrade and changes the client version every player needs; back up the PVC
first. GriefPrevention and Simple Voice Chat resolve from Modrinth at boot for
the pinned version, so a version bump can fail startup if either plugin has no
matching release yet; the pod log says so.

### Vintage Story

`infra/vintagestory.tf` runs the official Vintage Story server on `agent-2`'s
port 42420, TCP and UDP, as pod hostPorts for the same reason as Minecraft.
The image is ours, `docker/vintagestory/Dockerfile`: Anego's linux-x64 server
tarball with Anego's arm64 launcher and native libraries swapped in, built and
published for arm64 by `.github/workflows/vintagestory.yml` at the tag
`vintagestory_image` in `defaults.tf` names. One DNS record, by hand and
DNS-only like the rest:

```
vs.late.sh                A    <agent-2-ip>
```

The world lives on the `vintagestory-data` PVC (`local-path`,
`prevent_destroy`, reclaim policy Delete) on `agent-2`'s disk. Access is
account verification plus the server's whitelist. The server reads console
commands on stdin, so whitelisting is an attach, not an exec:

```bash
echo '/player <name> whitelist on' | kubectl attach -i -n default deploy/vintagestory
kubectl logs -n default deploy/vintagestory --tail=20    # the command's answer
```

`late-ssh/src/app/door/vintagestory/CONTEXT.md` has the rest: settings,
operations, and the upgrade checklist.

## Configuration Parameters

Application configuration does not live in Terraform. `service-ssh` and
`service-web` read `LATE_ENV=prod` plus secrets; everything else is compiled
into their `config.rs` profiles (`late-ssh/src/config.rs`,
`late-web/src/config.rs`). The variables below exist only for infrastructure
shape, images, and secrets, set as Terraform variables (via GitHub
secrets/variables for CI/CD).

### Core

| Variable       | Description                                                                                                                                                                          |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `LOG_LEVEL`    | Rust log level (`RUST_LOG`)                                                                                                                                                          |
| `SSH_HOST_KEY` | Ed25519 private key for SSH server                                                                                                                                                   |
| `IMAGE_TAGS`   | Component name -> image ref map (ssh, web, and each door). Only read when a deployment is created; deploys go through `kubectl set image` and every deployment ignores image changes |

### IRC

The IRC edge is always provisioned: Terraform requests a Let's Encrypt
certificate for `irc.late.sh` with cert-manager, mounts the generated Kubernetes
TLS secret into `service-ssh`, and exposes port 6697 through `ssh-edge`. The
listener itself (ports, limits, TLS paths, trusted proxy CIDRs) is part of the
late-ssh prod profile.

| Variable         | Description                                                         |
| ---------------- | ------------------------------------------------------------------- |
| `IRC_PROXY_EMIT` | Make the `ssh-edge` IRC backend emit PROXY headers, defaults to `0` |

`IRC_PROXY_EMIT` stays a variable because flipping edge emission is a
deploy-time rollout step: deploy a parser-capable image first, then set the
GitHub environment variable `IRC_PROXY_EMIT=1` and run a subsequent
infrastructure deployment. Rollback reverses the order. The prod profile always
accepts PROXY headers, so there is no accept-side toggle.

### IPv6 edge proxy

| Variable             | Description                                          |
| -------------------- | ---------------------------------------------------- |
| `IPV6_PROXY_ENABLED` | Deploy the host-network IPv6-only HAProxy edge proxy |
| `IPV6_PROXY_ADDRESS` | Public IPv6 address for the proxy to bind            |
| `IPV6_PROXY_IMAGE`   | HAProxy image used by the proxy                      |

### Secrets injected into late-ssh

| Variable          | Description                                                                 |
| ----------------- | --------------------------------------------------------------------------- |
| `AI_API_KEY`      | Gemini API key                                                              |
| `YOUTUBE_API_KEY` | YouTube Data API key                                                        |
| `HERALD_TOKEN`    | Mastodon application token for the herald (`write:media`, `write:statuses`) |

### Voice / LiveKit

| Variable                | Description                                                                   |
| ----------------------- | ----------------------------------------------------------------------------- |
| `LIVEKIT_IMAGE`         | LiveKit server image                                                          |
| `LIVEKIT_LOG_LEVEL`     | LiveKit server log level                                                      |
| `LIVEKIT_API_KEY`       | LiveKit API key; API secret is generated into the Kubernetes `livekit` secret |
| `LIVEKIT_RTC_TCP_PORT`  | ICE/TCP fallback port, default `7881`                                         |
| `LIVEKIT_RTC_UDP_PORT`  | ICE/UDP mux port, default `7882`                                              |
| `LIVEKIT_TURN_ENABLED`  | Enable embedded TURN/STUN, default `true`                                     |
| `LIVEKIT_TURN_UDP_PORT` | TURN/STUN UDP port, default `3478`                                            |
| `LIVEKIT_TURN_TLS_PORT` | TURN/TLS TCP port, default `5349`                                             |

### Minecraft

| Variable              | Description                                                                                                 |
| --------------------- | ----------------------------------------------------------------------------------------------------------- |
| `MINECRAFT_WHITELIST` | Comma-separated usernames allowed to join, seeded on every boot. Empty seeds nobody; add names via rcon-cli |
| `MINECRAFT_OPS`       | Comma-separated usernames granted operator, seeded on every boot                                            |

Image, game version, heap, and port are locals in `defaults.tf`, not variables:
a version bump is a reviewed diff. Vintage Story has no variables at all: its
image and port are `defaults.tf` locals too, and its whitelist lives on its
volume.

### S3 Storage

| Variable               | Description                                                                |
| ---------------------- | -------------------------------------------------------------------------- |
| `S3_ACCESS_KEY_ID`     | S3 access key (DB backups and file uploads)                                |
| `S3_SECRET_ACCESS_KEY` | S3 secret key (DB backups and file uploads)                                |
| `S3_ENDPOINT`          | S3 endpoint URL (DB backups; the files endpoint is a prod-profile literal) |
| `DB_BACKUPS_BUCKET`    | Bucket for CloudNativePG backups                                           |

## Infrastructure Checks

PRs touching `infra/` run `terraform fmt -check -recursive` and
`terraform validate`, initialized with `terraform init -backend=false -input=false`
so validation needs no cluster or state credentials. For same-repository PRs the
`terraform_plan` workflow also plans against live production state and rejects
planned destroys. It runs on PRs only, with no manual trigger. Fork PRs skip the
live plan because GitHub withholds environment secrets; to plan a fork's infra
change, push its branch to this repository and open the PR from there.

## Production Considerations

- Increase CloudNativePG instances from 2 to 3
- Replace `local-path-provisioner` with Longhorn for distributed storage
- Place a load balancer in front of the cluster
- Enable Cloudflare proxy for DDoS protection
- Increase resource limits for late-ssh (CPU-intensive TUI rendering)
