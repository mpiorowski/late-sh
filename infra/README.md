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
- `https://late.sh` — Web landing + audio pairing
- `https://api.late.sh` — SSH API / WebSocket
- `https://audio.late.sh` — Icecast audio stream
- `https://grafana.late.sh` — Monitoring

### 5. Set Up S3 Buckets

Create two buckets in your S3-compatible provider:
- `{context}-tf-state` — Terraform state
- `{context}-db-backups` — Database backups

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
| late-ssh | `service-ssh-sv` | 2222 (SSH), 4000 (API) | SSH TUI server + HTTP API |
| late-web | `service-web-sv` | 3000 | Web landing page + pairing |
| Icecast | `icecast-sv` | 8000 | Audio streaming server |
| Liquidsoap | `liquidsoap-sv` | 1234 (telnet) | Playlist manager + encoder |
| PostgreSQL | `postgres-rw` | 5432 | CloudNativePG cluster |
| Monitoring | OpenTelemetry Collector, VictoriaMetrics, VictoriaLogs, VictoriaTraces, Grafana | various | Full observability stack |

SSH traffic on port 22 is routed via NGINX TCP passthrough to late-ssh pod port 2222.

## Configuration Parameters

All parameters are set as Terraform variables (via GitHub secrets/variables for CI/CD).

### Core

| Variable | Description |
|----------|-------------|
| `DOMAIN` | Root domain (e.g., `late.sh`) |
| `LOG_LEVEL` | Rust log level (`RUST_LOG`) |
| `SSH_HOST_KEY` | Ed25519 private key for SSH server |
| `SSH_IMAGE_TAG` | Docker image for late-ssh |
| `WEB_IMAGE_TAG` | Docker image for late-web |

### SSH / Rate Limits

| Variable | Description |
|----------|-------------|
| `SSH_OPEN` | Allow open SSH access (no auth required) |
| `MAX_CONNS_GLOBAL` | Max total concurrent SSH connections |
| `MAX_CONNS_PER_IP` | Max concurrent SSH connections per IP |
| `SSH_IDLE_TIMEOUT` | SSH idle timeout in seconds |
| `FRAME_DROP_LOG_EVERY` | Log every Nth frame drop |
| `SSH_MAX_ATTEMPTS_PER_IP` | Max SSH attempts per IP in rate limit window |
| `SSH_RATE_LIMIT_WINDOW_SECS` | SSH rate limit window in seconds |
| `SSH_PROXY_PROTOCOL` | Enable PROXY protocol parsing for SSH client IP resolution |
| `SSH_PROXY_TRUSTED_CIDRS` | Comma-separated CIDRs trusted to send PROXY headers |
| `WS_PAIR_MAX_ATTEMPTS_PER_IP` | Max WebSocket pair attempts per IP in window |
| `WS_PAIR_RATE_LIMIT_WINDOW_SECS` | WebSocket pair rate limit window in seconds |
| `DB_POOL_SIZE` | Database connection pool size |

### IPv6 edge proxy

| Variable | Description |
|----------|-------------|
| `IPV6_PROXY_ENABLED` | Deploy the host-network IPv6-only HAProxy edge proxy |
| `IPV6_PROXY_ADDRESS` | Public IPv6 address for the proxy to bind |
| `IPV6_PROXY_IMAGE` | HAProxy image used by the proxy |

### AI (Gemini)

| Variable | Description |
|----------|-------------|
| `AI_ENABLED` | Enable AI features (ghost chat, URL extraction) |
| `AI_API_KEY` | Gemini API key |
| `AI_MODEL` | Gemini model name |

### Vote

| Variable | Description |
|----------|-------------|
| `VOTE_SWITCH_INTERVAL_SECS` | Vote round duration in seconds |

### S3 Storage

| Variable | Description |
|----------|-------------|
| `S3_ACCESS_KEY_ID` | S3 access key |
| `S3_SECRET_ACCESS_KEY` | S3 secret key |
| `S3_ENDPOINT` | S3 endpoint URL |
| `DB_BACKUPS_BUCKET` | Bucket for CloudNativePG backups |

## Production Considerations

- Increase CloudNativePG instances from 2 to 3
- Replace `local-path-provisioner` with Longhorn for distributed storage
- Place a load balancer in front of the cluster
- Enable Cloudflare proxy for DDoS protection
- Increase resource limits for late-ssh (CPU-intensive TUI rendering)
