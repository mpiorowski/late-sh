# =============================================================================
# Core Infrastructure Variables
# =============================================================================

variable "KUBE_CONFIG_PATH" {
  description = "Path to the kubeconfig file"
  type        = string
}

variable "DOCKER_CONFIG_JSON" {
  description = "The content of the .dockerconfigjson file."
  type        = string
  sensitive   = true
}

variable "LOG_LEVEL" {
  description = "Rust log level (RUST_LOG)."
  type        = string
}

variable "DOMAIN" {
  description = "The root domain (e.g., late.sh)."
  type        = string
}

variable "GRAFANA_URL" {
  description = "The URL for the Grafana dashboard."
  type        = string
}

# =============================================================================
# Service Images
# =============================================================================

variable "SSH_IMAGE_TAG" {
  description = "Docker image for late-ssh (e.g., ghcr.io/org/late-ssh:sha-abc123)."
  type        = string
}

variable "WEB_IMAGE_TAG" {
  description = "Docker image for late-web (e.g., ghcr.io/org/late-web:sha-abc123)."
  type        = string
}

# =============================================================================
# SSH Host Key
# =============================================================================

variable "SSH_HOST_KEY" {
  description = "Ed25519 private key for the SSH server (russh host key)."
  type        = string
  sensitive   = true
}

# =============================================================================
# SSH / Rate Limits
# =============================================================================

variable "SSH_OPEN" {
  description = "Allow open SSH access (no auth required)."
  type        = string
}

variable "MAX_CONNS_GLOBAL" {
  description = "Max total concurrent SSH connections."
  type        = string
}

variable "MAX_CONNS_PER_IP" {
  description = "Max concurrent SSH connections per IP."
  type        = string
}

variable "SSH_IDLE_TIMEOUT" {
  description = "SSH idle timeout in seconds."
  type        = string
}

variable "FRAME_DROP_LOG_EVERY" {
  description = "Log every Nth frame drop."
  type        = string
}

variable "SSH_MAX_ATTEMPTS_PER_IP" {
  description = "Max SSH connection attempts per IP in rate limit window."
  type        = string
}

variable "SSH_RATE_LIMIT_WINDOW_SECS" {
  description = "SSH rate limit window in seconds."
  type        = string
}

variable "SSH_PROXY_PROTOCOL" {
  description = "Enable PROXY protocol parsing in late-ssh."
  type        = string
  default     = "1"
}

variable "SSH_PROXY_TRUSTED_CIDRS" {
  description = "Comma-separated CIDRs trusted to send PROXY protocol headers."
  type        = string
  default     = "10.42.0.0/16,46.62.210.86/32"
}

# =============================================================================
# IPv6 edge proxy
# =============================================================================

variable "IPV6_PROXY_ENABLED" {
  description = "Deploy a host-network IPv6-only TCP proxy in front of the IPv4-only cluster ingress."
  type        = bool
  default     = true
}

variable "IPV6_PROXY_ADDRESS" {
  description = "Public IPv6 address to bind for the IPv6 edge proxy."
  type        = string
  default     = "2a01:4f9:c013:2ae1::1"
}

variable "IPV6_PROXY_IMAGE" {
  description = "HAProxy image used by the IPv6 edge proxy."
  type        = string
  default     = "haproxy:2.9-alpine"
}

variable "WS_PAIR_MAX_ATTEMPTS_PER_IP" {
  description = "Max WebSocket pair attempts per IP in rate limit window."
  type        = string
}

variable "WS_PAIR_RATE_LIMIT_WINDOW_SECS" {
  description = "WebSocket pair rate limit window in seconds."
  type        = string
}

variable "DB_POOL_SIZE" {
  description = "Database connection pool size."
  type        = string
}

# =============================================================================
# AI (Gemini)
# =============================================================================

variable "AI_API_KEY" {
  description = "Gemini API key for AI features (ghost chat, URL extraction)."
  type        = string
  sensitive   = true
}

variable "AI_MODEL" {
  description = "Gemini model name."
  type        = string
}

variable "AI_ENABLED" {
  description = "Enable AI features."
  type        = string
}

# =============================================================================
# Vote
# =============================================================================

variable "VOTE_SWITCH_INTERVAL_SECS" {
  description = "Vote round duration in seconds."
  type        = string
}

# =============================================================================
# S3-Compatible Storage (for DB backups)
# =============================================================================

variable "S3_ACCESS_KEY_ID" {
  description = "S3-compatible storage access key ID."
  type        = string
  sensitive   = true
}

variable "S3_SECRET_ACCESS_KEY" {
  description = "S3-compatible storage secret access key."
  type        = string
  sensitive   = true
}

variable "S3_ENDPOINT" {
  description = "S3-compatible storage endpoint URL."
  type        = string
}

variable "DB_BACKUPS_BUCKET" {
  description = "S3 bucket name for CloudNativePG backups."
  type        = string
}
