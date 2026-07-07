# =============================================================================
# late-dopewars: standalone dopewars door host (game served over SSH)
# =============================================================================
# Runs the real upstream dopewars curses client on a PTY per session and serves
# it over SSH. service-ssh reaches it as a network-proxied door (the same model
# as the nethack host); the dopewars child no longer runs inside the SSH service
# container. See late-ssh/src/app/door/dopewars/CONTEXT.md and the late-dopewars
# crate.
#
# Persistence: this pod owns the shared high-score file. It mounts the
# `dopewars-save` PVC (defined in dopewars.tf) at LATE_DOPEWARS_SCORE_FILE's
# parent dir, so the leaderboard survives restarts and is shared across players.
# dopewars has no mid-game save, so there is no getlock/SIGHUP dance -- teardown
# just kills the child.
#
# replicas MUST stay 1: one RWO volume holds the shared score file (see dopewars.tf
# for the single-node reasoning). The host pod is always deployed (like
# service-ssh/nethack); the door's enable flag only gates the CLIENT (service-ssh's
# LATE_DOPEWARS_ENABLED). Keeping the host unconditional means its image always
# exists in-cluster, so the deploy workflows can read it with a plain `kubectl get`
# (no bootstrap fallback) just like the ssh/web/nethack images.

resource "kubernetes_deployment_v1" "late_dopewars" {
  metadata {
    name = "late-dopewars"
  }

  spec {
    replicas = 1

    # Kill-before-create: the old pod fully terminates before the new one starts,
    # so the two never co-mount the RWO volume. Costs a few seconds of door
    # downtime per host redeploy, which is fine for a single-replica door.
    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 0
        max_unavailable = 1
      }
    }

    selector {
      match_labels = {
        app = "late-dopewars"
      }
    }

    template {
      metadata {
        labels = {
          app = "late-dopewars"
        }
      }

      spec {
        # Hand the shared score directory on the PVC to the `late` user before the
        # host starts (an empty PVC mount is root-owned and would shadow the
        # image's baked dir). dopewars creates the .sco file itself on the first
        # score write, so we only fix ownership. Idempotent; runs as root to chown.
        init_container {
          name  = "dopewars-score-seed"
          image = var.DOPEWARS_IMAGE_TAG
          command = [
            "sh", "-c",
            "mkdir -p ${local.dopewars_var_path} && chown -R late:late ${local.dopewars_var_path}",
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "dopewars-save"
            mount_path = local.dopewars_var_path
          }
        }

        container {
          image = var.DOPEWARS_IMAGE_TAG
          name  = "late-dopewars"

          port {
            container_port = 2324
            name           = "dopewars"
          }

          resources {
            limits = {
              cpu    = "2000m"
              memory = "1Gi"
            }
            requests = {
              cpu    = "250m"
              memory = "256Mi"
            }
          }

          startup_probe {
            tcp_socket {
              port = "dopewars"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "dopewars"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "dopewars"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 6
          }

          env {
            name  = "RUST_LOG"
            value = var.LOG_LEVEL
          }

          # Shared secret authorizing late-ssh -> this host (same value injected
          # into service-ssh as LATE_DOPEWARS_SECRET).
          env {
            name = "LATE_DOPEWARS_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.dopewars_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          # The single shared high-score file on the PVC.
          env {
            name  = "LATE_DOPEWARS_SCORE_FILE"
            value = local.dopewars_score_file
          }

          volume_mount {
            name       = "dopewars-save"
            mount_path = local.dopewars_var_path
          }
        }

        volume {
          name = "dopewars-save"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.dopewars_save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "late_dopewars_sv" {
  metadata {
    name = "late-dopewars-sv"
  }

  spec {
    selector = {
      app = "late-dopewars"
    }

    # Cluster-internal only: reached by service-ssh at late-dopewars-sv:2324. Not
    # exposed via ingress or the ssh-tcp LoadBalancer.
    port {
      name        = "dopewars"
      port        = 2324
      target_port = "dopewars"
    }
  }
}
