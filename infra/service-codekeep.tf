# Dedicated CodeKeep SSH/PTY host. The upstream Bun/Ink process never runs in
# service-ssh; this pod owns its runtime, per-account saves, and resource limits.
#
# replicas MUST stay 1: one RWO volume holds every account's save (see
# codekeep.tf), and the host's one-live-child-per-account lease is process-local,
# so a second replica would let two children race the same game.json.
# The host pod is always deployed (like service-ssh/nethack/dcss); the door's
# enable flag only gates the CLIENT (service-ssh's LATE_CODEKEEP_ENABLED).
resource "kubernetes_deployment_v1" "late_codekeep" {
  metadata {
    name = "late-codekeep"
  }

  spec {
    replicas = 1

    strategy {
      type = "RollingUpdate"
      rolling_update {
        # The RWO save volume has one owner; stop the old host before mounting it
        # in the replacement. Its shutdown grace SIGHUP-saves active games.
        max_surge       = 0
        max_unavailable = 1
      }
    }

    selector {
      match_labels = {
        app = "late-codekeep"
      }
    }

    template {
      metadata {
        labels = {
          app = "late-codekeep"
        }
      }

      spec {
        termination_grace_period_seconds = 30

        init_container {
          name  = "codekeep-save-seed"
          image = var.CODEKEEP_IMAGE_TAG
          command = [
            "sh", "-c",
            "mkdir -p ${local.codekeep_var_path} && chown -R late:late ${local.codekeep_var_path}",
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "codekeep-save"
            mount_path = local.codekeep_var_path
          }
        }

        container {
          image = var.CODEKEEP_IMAGE_TAG
          name  = "late-codekeep"

          port {
            container_port = 2328
            name           = "codekeep"
          }

          resources {
            limits = {
              cpu    = local.door_cpu_limit
              memory = local.door_memory_limit
            }
            requests = {
              cpu    = local.door_cpu_request
              memory = local.door_memory_request
            }
          }

          startup_probe {
            tcp_socket {
              port = "codekeep"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "codekeep"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "codekeep"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 6
          }

          env {
            name  = "RUST_LOG"
            value = var.LOG_LEVEL
          }

          env {
            name = "LATE_CODEKEEP_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.codekeep_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          env {
            name  = "LATE_CODEKEEP_DATA_DIR"
            value = local.codekeep_var_path
          }

          volume_mount {
            name       = "codekeep-save"
            mount_path = local.codekeep_var_path
          }
        }

        volume {
          name = "codekeep-save"
          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.codekeep_save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = "ghcr-pull-secret"
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "late_codekeep_sv" {
  metadata {
    name = "late-codekeep-sv"
  }

  spec {
    selector = {
      app = "late-codekeep"
    }

    port {
      name        = "codekeep"
      port        = 2328
      target_port = "codekeep"
    }

    type = "ClusterIP"
  }
}
