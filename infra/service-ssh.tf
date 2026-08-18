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
        termination_grace_period_seconds = 21600

        # Every door game runs in its own late-<game> pod (doors.tf), which
        # owns that game's save PVC + seed init_container. service-ssh only
        # needs network reach to them; the host addresses are late-ssh
        # config.rs prod-profile literals (late-<game>-sv).

        container {
          image = local.image_tags["ssh"]
          name  = "service-ssh"

          port {
            container_port = 2222
            name           = "ssh"
          }

          port {
            container_port = 4000
            name           = "api"
          }

          dynamic "port" {
            for_each = local.irc_enabled_bool ? [1] : []

            content {
              container_port = local.irc_port
              name           = "irc"
            }
          }

          resources {
            limits = {
              cpu    = "8000m"
              memory = "8Gi"
            }
            requests = {
              cpu    = "1000m"
              memory = "2Gi"
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
          # Per-pod telemetry identity: without service.instance.id every pod
          # exports the same otel series and they clobber each other on scrape
          # (fatal once service-ssh runs multiple replicas). $(POD_NAME) is the
          # downward-API pod name; the SDK's env resource detector reads
          # OTEL_RESOURCE_ATTRIBUTES, the collector turns it into a metric label.
          env {
            name = "POD_NAME"
            value_from {
              field_ref {
                field_path = "metadata.name"
              }
            }
          }
          env {
            name  = "OTEL_RESOURCE_ATTRIBUTES"
            value = "service.instance.id=$(POD_NAME)"
          }
          # Selects the config.rs profile; every non-secret value lives there.
          env {
            name  = "LATE_ENV"
            value = "prod"
          }

          # --- Database (CloudNativePG operator-generated credentials) ---
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

          # --- Door games (shared identity secrets; targets live in config.rs) ---
          env {
            name = "LATE_REBELS_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.rebels_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_NETHACK_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["nethack"].identity_secret_name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_DOPEWARS_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["dopewars"].identity_secret_name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_CODEKEEP_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["codekeep"].identity_secret_name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_DCSS_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["dcss"].identity_secret_name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_BASHQUEST_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["bashquest"].identity_secret_name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_BROGUE_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["brogue"].identity_secret_name
                key  = "secret"
              }
            }
          }

          env {
            name = "LATE_USURPER_SECRET"
            value_from {
              secret_key_ref {
                name = module.door["usurper"].identity_secret_name
                key  = "secret"
              }
            }
          }

          # --- Files / uploads (R2 credentials; endpoint and bucket live in config.rs) ---
          env {
            name = "LATE_FILES_S3_ACCESS_KEY_ID"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.s3_credentials.metadata[0].name
                key  = "ACCESS_KEY_ID"
              }
            }
          }
          env {
            name = "LATE_FILES_S3_SECRET_ACCESS_KEY"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.s3_credentials.metadata[0].name
                key  = "SECRET_ACCESS_KEY"
              }
            }
          }

          # --- AI ---
          env {
            name = "LATE_AI_API_KEY"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.ai_credentials.metadata[0].name
                key  = "api_key"
              }
            }
          }
          # --- YouTube Data API ---
          env {
            name = "LATE_YOUTUBE_API_KEY"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.youtube_credentials.metadata[0].name
                key  = "api_key"
              }
            }
          }

          # --- Voice / LiveKit (credentials only; both URLs and the room name
          # are prod-profile literals in late-ssh/src/config.rs, including the
          # cluster-internal Twirp base http://livekit-sv) ---
          env {
            name = "LATE_LIVEKIT_API_KEY"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.livekit.metadata[0].name
                key  = "api_key"
              }
            }
          }
          env {
            name = "LATE_LIVEKIT_API_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.livekit.metadata[0].name
                key  = "api_secret"
              }
            }
          }

          # --- SSH host key volume ---
          volume_mount {
            name       = "ssh-host-key"
            mount_path = "/app/keys"
            read_only  = true
          }

          dynamic "volume_mount" {
            for_each = local.irc_enabled_bool ? [1] : []

            content {
              name       = "irc-tls"
              mount_path = local.irc_tls_mount_path
              read_only  = true
            }
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

        dynamic "volume" {
          for_each = local.irc_enabled_bool ? [1] : []

          content {
            name = "irc-tls"

            secret {
              secret_name = local.irc_tls_secret_name
            }
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }

  # Images are deployed with `kubectl set image` (deploy_service.yml), never
  # by terraform applies, so a full apply must not roll the service back to
  # whatever tag it was created with.
  lifecycle {
    ignore_changes = [
      spec[0].template[0].spec[0].container[0].image,
    ]
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

    dynamic "port" {
      for_each = local.irc_enabled_bool ? [1] : []

      content {
        name        = "irc"
        port        = local.irc_port
        target_port = "irc"
      }
    }
  }
}
