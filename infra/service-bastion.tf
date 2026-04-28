# =============================================================================
# late-bastion: thin SSH frontend that tunnels to late-ssh /tunnel over WS.
#
# Intentionally minimal — no DB, no per-user state, no ban logic. See
# PERSISTENT-CONNECTION-GATEWAY.md §5 for the "no smarter than it needs
# to be to connect the wires" principle. Rolling updates here drop every
# active SSH session, so this Deployment is expected to update rarely.
#
# Ports: 5222 (russh; user-facing during dual-path rollout). Cutover to
# :22 (post Phase 5) is a one-line edit in infra/ssh-tcp.tf.
# =============================================================================

resource "kubernetes_deployment_v1" "service_bastion" {
  count = var.BASTION_ENABLED == "1" ? 1 : 0

  metadata {
    name = "service-bastion"
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
        app = "service-bastion"
      }
    }

    template {
      metadata {
        labels = {
          app = "service-bastion"
        }
      }

      spec {
        # Bastion upgrades drop sessions; give existing sessions time to
        # observe and reconnect to a new pod cleanly.
        termination_grace_period_seconds = 7200

        container {
          image = var.BASTION_IMAGE_TAG
          name  = "service-bastion"

          port {
            container_port = 5222
            name           = "ssh"
          }

          resources {
            limits = {
              cpu    = "1000m"
              memory = "512Mi"
            }
            requests = {
              cpu    = "100m"
              memory = "64Mi"
            }
          }

          # russh listener TCP probe — bastion has no HTTP surface.
          startup_probe {
            tcp_socket {
              port = "ssh"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "ssh"
            }
            initial_delay_seconds = 30
            period_seconds        = 30
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "ssh"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 3
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

          # --- Bastion config ---
          env {
            name  = "LATE_BASTION_SSH_PORT"
            value = var.BASTION_SSH_PORT
          }
          env {
            name  = "LATE_BASTION_HOST_KEY_PATH"
            value = "/app/keys/host_key"
          }
          env {
            name  = "LATE_BASTION_SSH_IDLE_TIMEOUT"
            value = var.BASTION_SSH_IDLE_TIMEOUT
          }
          env {
            name  = "LATE_BASTION_BACKEND_TUNNEL_URL"
            value = "ws://service-ssh-internal-sv:4001/tunnel"
          }
          env {
            name = "LATE_BASTION_SHARED_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.bastion_shared_secret.metadata[0].name
                key  = "secret"
              }
            }
          }
          env {
            name  = "LATE_BASTION_MAX_CONNS_GLOBAL"
            value = var.BASTION_MAX_CONNS_GLOBAL
          }
          env {
            name  = "LATE_BASTION_PROXY_PROTOCOL"
            value = var.SSH_PROXY_PROTOCOL
          }
          env {
            name  = "LATE_BASTION_PROXY_TRUSTED_CIDRS"
            value = var.SSH_PROXY_TRUSTED_CIDRS
          }

          # --- Bastion russh host key volume ---
          volume_mount {
            name       = "bastion-host-key"
            mount_path = "/app/keys"
            read_only  = true
          }
        }

        volume {
          name = "bastion-host-key"

          secret {
            secret_name = kubernetes_secret_v1.bastion_host_key.metadata[0].name

            items {
              key  = "host_key"
              path = "host_key"
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

resource "kubernetes_service_v1" "service_bastion_sv" {
  count = var.BASTION_ENABLED == "1" ? 1 : 0

  metadata {
    name = "service-bastion-sv"
  }

  spec {
    selector = {
      app = "service-bastion"
    }

    port {
      name        = "ssh"
      port        = 5222
      target_port = "ssh"
    }
  }
}
