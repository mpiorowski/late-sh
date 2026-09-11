# late.sh Infra

## Infrastructure Setup

Follow these steps to provision the infrastructure for late.sh.

### Prerequisites

You need at least one Linux server (VPS or bare metal) with:
- **OS:** Debian 12+, Ubuntu 22.04+, RHEL 9+, or any [RKE2-supported distro](https://docs.rke2.io/install/requirements#operating-systems)
- **Arch:** x86_64 or aarch64
- **CPU:** 4 vCPUs
- **RAM:** 8 GB
- **Disk:** 40 GB+
- **SSH access** with a key pair

Providers like Hetzner, DigitalOcean, or AWS EC2 all work. For HA, provision 2-3 server nodes.

### 1. Set Up Local Environment

```bash
cp .env.example .env
```

Edit `infra/.env` with your server details (IP, SSH user, key path, name).

### 2. Set Up Kubernetes Cluster (RKE2)

```bash
sh setup_rke2.sh
```

Installs RKE2, configures kubeconfig, and creates the `staging` GitHub environment.

### 3. Configure Application

```bash
gh auth login -s write:packages
sh setup_app.sh
```

You'll be prompted for:
- **Domain** (default: `late.sh`)
- **S3-compatible storage** — endpoint, access key, secret key for TF state and DB backups
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
forwards traffic into the existing IPv4 ingress path.

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

For Cloudflare R2, attach a custom domain such as `files.<domain>` to the
files bucket and set `FILES_PUBLIC_BASE_URL` to that exact public base URL.

### 6. Deploy

Create a release to trigger CI/CD:

```bash
# Staging
gh release create v0.1.0-rc --prerelease --title "Staging" --notes "Initial deployment"

# Production
gh release create v1.0.0 --title "Production" --notes "Initial deployment"
```

After the monitoring stack is deployed, retrieve the generated Grafana admin password:

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

| Component | Service | Ports | Description |
|-----------|---------|-------|-------------|
| late-ssh | `service-ssh-sv` | 2222 (SSH), 4000 (API), 6697 (IRC TLS when enabled) | SSH TUI server + HTTP API + embedded IRC |
| late-web | `service-web-sv` | 3000 | Web landing page + pairing |
| Icecast | `icecast-sv` | 8000 | Audio streaming server |
| Liquidsoap | none (dials out to `icecast-sv`) | - | Playlist encoder |
| LiveKit | `livekit-sv` | 7880 (WSS/API), 7881 TCP, 7882 UDP, 3478 UDP, 5349 TCP | Voice-room SFU, ICE/TURN media |
| Minecraft | none (hostPort) | 25565 TCP on the node | Paper server with GriefPrevention, whitelist-only |
| PostgreSQL | `postgres-rw` | 5432 | CloudNativePG cluster |
| Monitoring | OpenTelemetry Collector, VictoriaMetrics, VictoriaLogs, VictoriaTraces, Grafana | various | Full observability stack |

SSH traffic on port 22 is routed via NGINX TCP passthrough to late-ssh pod port 2222.
IRC traffic follows the same TCP passthrough pattern when enabled. Both NGINX
for IPv4 and the host-network HAProxy for IPv6 send PROXY v1 metadata before the
application's in-process TLS handshake; late-ssh accepts it only from the CIDRs
configured by `SSH_PROXY_TRUSTED_CIDRS`.
LiveKit signaling is routed through NGINX ingress on `rtc.<domain>`, while
LiveKit media ports are bound directly on the node by the `livekit` pod.
On a fresh cluster, the `livekit` pod may wait for cert-manager to create the
`livekit-tls` secret used by embedded TURN/TLS. If it sits in
`ContainerCreating`, check certificate issuance before treating the rollout as
failed.

### Minecraft

`infra/minecraft.tf` runs a Paper server on the node's port 25565 as a pod
hostPort, deliberately not through the ingress-nginx TCP map: nginx reloads
drop long-lived TCP sessions, which would kick every player on each
cert-manager renewal. Players connect to `late.sh` (client default port).
The world lives on the `minecraft-data` PVC (`local-path`, `prevent_destroy`);
`worldborder set 6000` at startup caps its disk growth on the shared node disk.

Access is online-mode plus an enforced whitelist. `MINECRAFT_WHITELIST` and
`MINECRAFT_OPS` seed the lists on every boot. Day-to-day changes go through
rcon inside the pod and persist across restarts alongside the seeded names:

```bash
kubectl exec -n default deploy/minecraft -- rcon-cli whitelist add <name>
kubectl exec -n default deploy/minecraft -- rcon-cli whitelist remove <name>
kubectl exec -n default deploy/minecraft -- rcon-cli op <name>
kubectl exec -n default deploy/minecraft -- rcon-cli list
kubectl logs -n default deploy/minecraft --tail=200
```

Bumping the game version (`minecraft_version` in `defaults.tf`) is a one-way
world upgrade and changes the client version every player needs; back up the
PVC first. GriefPrevention resolves from Modrinth at boot for the pinned
version, so a version bump can fail startup if the plugin has no matching
release yet; the pod log says so.

## Configuration Parameters

Application configuration does not live in Terraform. `service-ssh` and
`service-web` read `LATE_ENV=prod` plus secrets; everything else is compiled
into their `config.rs` profiles (`late-ssh/src/config.rs`,
`late-web/src/config.rs`). The variables below exist only for infrastructure
shape, images, and secrets, set as Terraform variables (via GitHub
secrets/variables for CI/CD).

### Core

| Variable | Description |
|----------|-------------|
| `LOG_LEVEL` | Rust log level (`RUST_LOG`) |
| `SSH_HOST_KEY` | Ed25519 private key for SSH server |
| `IMAGE_TAGS` | Component name -> image ref map (ssh, web, and each door). Only read when a deployment is created; deploys go through `kubectl set image` and every deployment ignores image changes |

### IRC

The IRC edge is always provisioned: Terraform requests a Let's Encrypt
certificate for `irc.late.sh` with cert-manager, mounts the generated
Kubernetes TLS secret into `service-ssh`, and exposes port 6697 through
ingress. The listener itself (ports, limits, TLS paths, trusted proxy CIDRs)
is part of the late-ssh prod profile.

| Variable | Description |
|----------|-------------|
| `IRC_PROXY_EMIT` | Make ingress-nginx and IPv6 HAProxy emit PROXY headers, defaults to `0` |

`IRC_PROXY_EMIT` stays a variable because flipping edge emission is a
deploy-time rollout step: deploy a parser-capable image first, then set the
GitHub environment variable `IRC_PROXY_EMIT=1` and run a subsequent
infrastructure deployment. Rollback reverses the order. The prod profile
always accepts PROXY headers, so the old accept-side toggle is gone.

### IPv6 edge proxy

| Variable | Description |
|----------|-------------|
| `IPV6_PROXY_ENABLED` | Deploy the host-network IPv6-only HAProxy edge proxy |
| `IPV6_PROXY_ADDRESS` | Public IPv6 address for the proxy to bind |
| `IPV6_PROXY_IMAGE` | HAProxy image used by the proxy |

### Secrets injected into late-ssh

| Variable | Description |
|----------|-------------|
| `AI_API_KEY` | Gemini API key |
| `YOUTUBE_API_KEY` | YouTube Data API key |

### Voice / LiveKit

| Variable | Description |
|----------|-------------|
| `LIVEKIT_IMAGE` | LiveKit server image |
| `LIVEKIT_LOG_LEVEL` | LiveKit server log level |
| `LIVEKIT_API_KEY` | LiveKit API key; API secret is generated into the Kubernetes `livekit` secret |
| `LIVEKIT_RTC_TCP_PORT` | ICE/TCP fallback port, default `7881` |
| `LIVEKIT_RTC_UDP_PORT` | ICE/UDP mux port, default `7882` |
| `LIVEKIT_TURN_ENABLED` | Enable embedded TURN/STUN, default `true` |
| `LIVEKIT_TURN_UDP_PORT` | TURN/STUN UDP port, default `3478` |
| `LIVEKIT_TURN_TLS_PORT` | TURN/TLS TCP port, default `5349` |

### Minecraft

| Variable | Description |
|----------|-------------|
| `MINECRAFT_WHITELIST` | Comma-separated usernames allowed to join, seeded on every boot. Empty seeds nobody; add names via rcon-cli |
| `MINECRAFT_OPS` | Comma-separated usernames granted operator, seeded on every boot |

Image, game version, heap, and port are locals in `defaults.tf`, not
variables: a version bump is a reviewed diff.

### S3 Storage

| Variable | Description |
|----------|-------------|
| `S3_ACCESS_KEY_ID` | S3 access key (DB backups and file uploads) |
| `S3_SECRET_ACCESS_KEY` | S3 secret key (DB backups and file uploads) |
| `S3_ENDPOINT` | S3 endpoint URL (DB backups; the files endpoint is a prod-profile literal) |
| `DB_BACKUPS_BUCKET` | Bucket for CloudNativePG backups |

## Production Considerations

- Increase CloudNativePG instances from 2 to 3
- Replace `local-path-provisioner` with Longhorn for distributed storage
- Place a load balancer in front of the cluster
- Enable Cloudflare proxy for DDoS protection
- Increase resource limits for late-ssh (CPU-intensive TUI rendering)
