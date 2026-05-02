# =============================================================================
# Container Registry Credentials
# =============================================================================

resource "kubernetes_secret_v1" "regcred" {
  metadata {
    name = "regcred"
  }

  data = {
    ".dockerconfigjson" = var.DOCKER_CONFIG_JSON
  }

  type = "kubernetes.io/dockerconfigjson"
}

# =============================================================================
# S3 Credentials (for CloudNativePG backups)
# =============================================================================

resource "kubernetes_secret_v1" "s3_credentials" {
  metadata {
    name = "s3-credentials"
  }

  data = {
    ACCESS_KEY_ID     = var.S3_ACCESS_KEY_ID
    SECRET_ACCESS_KEY = var.S3_SECRET_ACCESS_KEY
  }

  type = "Opaque"
}

# Note: CloudNativePG auto-generates db credentials in secret "postgres-app"
# The service-ssh deployment references that secret directly

# =============================================================================
# SSH Host Key
# =============================================================================

resource "kubernetes_secret_v1" "ssh_host_key" {
  metadata {
    name = "ssh-host-key"
  }

  data = {
    server_key = var.SSH_HOST_KEY
  }

  type = "Opaque"
}

# =============================================================================
# AI Credentials (Gemini)
# =============================================================================

resource "kubernetes_secret_v1" "ai_credentials" {
  metadata {
    name = "ai-credentials"
  }

  data = {
    api_key = var.AI_API_KEY
  }

  type = "Opaque"
}

# =============================================================================
# Web Terminal Tunnel Token
# =============================================================================

resource "random_password" "web_tunnel_token" {
  length  = 32
  special = false
}

resource "kubernetes_secret_v1" "web_tunnel_token" {
  metadata {
    name = "web-tunnel-token"
  }

  data = {
    token = random_password.web_tunnel_token.result
  }

  type = "Opaque"
}

# =============================================================================
# Icecast Passwords
# =============================================================================

resource "random_password" "icecast_admin" {
  length  = 32
  special = false
}

resource "random_password" "icecast_source" {
  length  = 32
  special = false
}

resource "random_password" "icecast_relay" {
  length  = 32
  special = false
}

# =============================================================================
# Grafana Admin Credentials
# =============================================================================

resource "random_password" "grafana_admin" {
  length           = 32
  special          = true
  override_special = "_%@"
}

resource "kubernetes_secret_v1" "grafana_admin" {
  metadata {
    name      = "grafana-admin"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  data = {
    username = "admin"
    password = random_password.grafana_admin.result
  }

  type = "Opaque"
}
