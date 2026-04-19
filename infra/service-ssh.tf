# =============================================================================
# late-ssh: SSH TUI server + HTTP API
# Ports: 2222 (SSH), 4000 (HTTP API)
# =============================================================================

resource "kubernetes_deployment_v1" "service_ssh" {
  metadata {
    name = "service-ssh"
  }

  spec {
    replicas = 1

    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 1
        max_unavailable = 0
      }
    }

    selector {
      match_labels = {
        app = "service-ssh"
      }
    }

    template {
      metadata {
        labels = {
          app = "service-ssh"
        }
      }

      spec {
        termination_grace_period_seconds = 7200

        container {
          image = var.SSH_IMAGE_TAG
          name  = "service-ssh"

          port {
            container_port = 2222
            name           = "ssh"
          }

          port {
            container_port = 4000
            name           = "api"
          }

          resources {
            limits = {
              cpu    = "4000m"
              memory = "2Gi"
            }
            requests = {
              cpu    = "1000m"
              memory = "512Mi"
            }
          }

          startup_probe {
            tcp_socket {
              port = "api"
            }
            initial_delay_seconds = 10
            period_seconds        = 10
            failure_threshold     = 30
          }

          liveness_probe {
            tcp_socket {
              port = "api"
            }
            initial_delay_seconds = 60
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            http_get {
              path = "/api/health"
              port = "api"
            }
            initial_delay_seconds = 15
            period_seconds        = 10
            failure_threshold     = 6
          }

          # --- Core ---
          env {
            name  = "RUST_LOG"
            value = var.LOG_LEVEL
          }
          env {
            name  = "OTEL_EXPORTER_OTLP_ENDPOINT"
            value = "http://otel-collector.monitoring.svc.cluster.local:4317"
          }
          env {
            name  = "LATE_SSH_PORT"
            value = "2222"
          }
          env {
            name  = "LATE_API_PORT"
            value = "4000"
          }

          # --- Database (CloudNativePG) ---
          env {
            name  = "LATE_DB_HOST"
            value = "postgres-rw"
          }
          env {
            name  = "LATE_DB_PORT"
            value = "5432"
          }
          env {
            name = "LATE_DB_NAME"
            value_from {
              secret_key_ref {
                name = "postgres-app"
                key  = "dbname"
              }
            }
          }
          env {
            name = "LATE_DB_USER"
            value_from {
              secret_key_ref {
                name = "postgres-app"
                key  = "user"
              }
            }
          }
          env {
            name = "LATE_DB_PASSWORD"
            value_from {
              secret_key_ref {
                name = "postgres-app"
                key  = "password"
              }
            }
          }

          # --- Audio ---
          env {
            name  = "LATE_ICECAST_URL"
            value = "http://icecast-sv:8000"
          }
          env {
            name  = "LATE_LIQUIDSOAP_ADDR"
            value = "liquidsoap-sv:1234"
          }

          # --- Web / CORS ---
          env {
            name  = "LATE_WEB_URL"
            value = "https://${var.DOMAIN}"
          }
          env {
            name  = "LATE_ALLOWED_ORIGINS"
            value = "https://${var.DOMAIN}"
          }

          # --- SSH ---
          env {
            name  = "LATE_SSH_KEY_PATH"
            value = "/app/keys/server_key"
          }
          env {
            name  = "LATE_SSH_OPEN"
            value = var.SSH_OPEN
          }
          env {
            name  = "LATE_FORCE_ADMIN"
            value = "0"
          }
          env {
            name  = "LATE_MAX_CONNS_GLOBAL"
            value = var.MAX_CONNS_GLOBAL
          }
          env {
            name  = "LATE_MAX_CONNS_PER_IP"
            value = var.MAX_CONNS_PER_IP
          }
          env {
            name  = "LATE_SSH_IDLE_TIMEOUT"
            value = var.SSH_IDLE_TIMEOUT
          }
          env {
            name  = "LATE_FRAME_DROP_LOG_EVERY"
            value = var.FRAME_DROP_LOG_EVERY
          }
          env {
            name  = "LATE_SSH_MAX_ATTEMPTS_PER_IP"
            value = var.SSH_MAX_ATTEMPTS_PER_IP
          }
          env {
            name  = "LATE_SSH_RATE_LIMIT_WINDOW_SECS"
            value = var.SSH_RATE_LIMIT_WINDOW_SECS
          }
          env {
            name  = "LATE_SSH_PROXY_PROTOCOL"
            value = var.SSH_PROXY_PROTOCOL
          }
          env {
            name  = "LATE_SSH_PROXY_TRUSTED_CIDRS"
            value = var.SSH_PROXY_TRUSTED_CIDRS
          }
          env {
            name  = "LATE_WS_PAIR_MAX_ATTEMPTS_PER_IP"
            value = var.WS_PAIR_MAX_ATTEMPTS_PER_IP
          }
          env {
            name  = "LATE_WS_PAIR_RATE_LIMIT_WINDOW_SECS"
            value = var.WS_PAIR_RATE_LIMIT_WINDOW_SECS
          }
          env {
            name  = "LATE_DB_POOL_SIZE"
            value = var.DB_POOL_SIZE
          }

          # --- Vote ---
          env {
            name  = "LATE_VOTE_SWITCH_INTERVAL_SECS"
            value = var.VOTE_SWITCH_INTERVAL_SECS
          }

          # --- AI ---
          env {
            name  = "LATE_AI_ENABLED"
            value = var.AI_ENABLED
          }
          env {
            name = "LATE_AI_API_KEY"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.ai_credentials.metadata[0].name
                key  = "api_key"
              }
            }
          }
          env {
            name  = "LATE_AI_MODEL"
            value = var.AI_MODEL
          }

          # --- SSH host key volume ---
          volume_mount {
            name       = "ssh-host-key"
            mount_path = "/app/keys"
            read_only  = true
          }
        }

        volume {
          name = "ssh-host-key"

          secret {
            secret_name = kubernetes_secret_v1.ssh_host_key.metadata[0].name

            items {
              key  = "server_key"
              path = "server_key"
              mode = "0444"
            }
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "service_ssh_sv" {
  metadata {
    name = "service-ssh-sv"
  }

  spec {
    selector = {
      app = "service-ssh"
    }

    port {
      name        = "ssh"
      port        = 2222
      target_port = "ssh"
    }

    port {
      name        = "api"
      port        = 4000
      target_port = "api"
    }
  }
}
