# =============================================================================
# Core Infrastructure Variables
# =============================================================================
# App configuration is NOT declared here. late-ssh reads LATE_ENV plus secrets;
# every other app value is compiled into late-ssh/src/config.rs. Variables in
# this file exist only for infrastructure shape, images, and secrets.

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

variable "NETHACK_IMAGE_TAG" {
  description = "Docker image for late-nethack, the NetHack door host (e.g., ghcr.io/org/late-nethack:sha-abc123)."
  type        = string
}

variable "DOPEWARS_IMAGE_TAG" {
  description = "Docker image for late-dopewars, the dopewars door host (e.g., ghcr.io/org/late-dopewars:sha-abc123)."
  type        = string
}

variable "CODEKEEP_IMAGE_TAG" {
  description = "Docker image for late-codekeep, the CodeKeep door host."
  type        = string
}

variable "DCSS_IMAGE_TAG" {
  description = "Docker image for late-dcss, the DCSS door host (e.g., ghcr.io/org/late-dcss:sha-abc123)."
  type        = string
}

variable "BROGUE_IMAGE_TAG" {
  description = "Docker image for late-brogue, the Brogue door host (e.g., ghcr.io/org/late-brogue:sha-abc123)."
  type        = string
}

variable "USURPER_IMAGE_TAG" {
  description = "Docker image for late-usurper, the Usurper door host (e.g., ghcr.io/org/late-usurper:sha-abc123)."
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

# =============================================================================
# Secrets injected into late-ssh
# =============================================================================

variable "AI_API_KEY" {
  description = "Gemini API key for AI features (ghost chat, URL extraction)."
  type        = string
  sensitive   = true
}

variable "YOUTUBE_API_KEY" {
  description = "YouTube Data API key for queue submit validation."
  type        = string
  sensitive   = true
}

# =============================================================================
# Voice / LiveKit
# =============================================================================

variable "LIVEKIT_IMAGE" {
  description = "LiveKit server image."
  type        = string
  default     = ""
}

variable "LIVEKIT_LOG_LEVEL" {
  description = "LiveKit server log level."
  type        = string
  default     = ""
}

variable "LIVEKIT_API_KEY" {
  description = "LiveKit API key used by late-ssh for token minting."
  type        = string
  default     = ""
}

variable "LIVEKIT_INGRESS_IMAGE" {
  description = "LiveKit ingress service image (WHIP ingest for OBS streams)."
  type        = string
  default     = ""
}

variable "LIVEKIT_INGRESS_WHIP_PORT" {
  description = "LiveKit ingress WHIP HTTP port (behind the nginx ingress)."
  type        = string
  default     = ""
}

variable "LIVEKIT_RTC_TCP_PORT" {
  description = "LiveKit ICE/TCP fallback port exposed directly on the node."
  type        = string
  default     = ""
}

variable "LIVEKIT_RTC_UDP_PORT" {
  description = "LiveKit ICE/UDP mux port exposed directly on the node."
  type        = string
  default     = ""
}

variable "LIVEKIT_RTC_USE_EXTERNAL_IP" {
  description = "Let LiveKit discover and advertise the node public IP for RTC candidates."
  type        = string
  default     = ""
}

variable "LIVEKIT_TURN_ENABLED" {
  description = "Enable LiveKit's embedded TURN/STUN service."
  type        = string
  default     = ""
}

variable "LIVEKIT_TURN_UDP_PORT" {
  description = "LiveKit embedded TURN/STUN UDP port exposed directly on the node."
  type        = string
  default     = ""
}

variable "LIVEKIT_TURN_TLS_PORT" {
  description = "LiveKit embedded TURN/TLS port exposed directly on the node."
  type        = string
  default     = ""
}

# =============================================================================
# IRC
# =============================================================================

variable "IRC_PROXY_EMIT" {
  description = "Make the IRC ingress proxies emit PROXY protocol headers. Enable only after the parser-capable image is deployed."
  type        = string
  default     = ""

  validation {
    condition     = contains(["", "0", "1", "true", "false", "yes", "no", "on", "off"], lower(trimspace(var.IRC_PROXY_EMIT)))
    error_message = "IRC_PROXY_EMIT must be a boolean-like string: 1/0, true/false, yes/no, or on/off."
  }
}

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
