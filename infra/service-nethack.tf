# =============================================================================
# late-nethack: standalone NetHack door host (game served over SSH)
# =============================================================================
# Runs the real upstream NetHack binary on a PTY per session and serves it over
# SSH. service-ssh reaches it as a network-proxied door (the rebels-camp model);
# the NetHack child no longer runs inside the SSH service container. See
# late-ssh/src/app/door/nethack/CONTEXT.md and the late-nethack crate.
#
# Persistence: this pod owns the writable playground. It mounts the existing
# `nethack-save` PVC (defined in nethack.tf) at VAR_PLAYGROUND, so per-player
# saves and shared bones carry over from when the door ran inside service-ssh.
# The nethack-save-seed init_container creates save/ + the append-only record
# files and hands the tree to the `late` user before the host starts (an empty
# PVC shadows the image's baked seed, and NetHack never mkdir's save/ itself).
#
# replicas MUST stay 1: one RWO volume holds the shared bones + per-player saves
# (see nethack.tf for the single-node / lock-file reasoning, which carries over).
# The host pod is always deployed (like service-ssh/web); the door's enable flag
# only gates the CLIENT (service-ssh's LATE_NETHACK_ENABLED, which shows the door
# available/unavailable to users). Keeping the host unconditional means its image
# always exists in-cluster, so the deploy workflows can read it with a plain
# `kubectl get` (no bootstrap fallback) just like the ssh/web images.

resource "kubernetes_deployment_v1" "late_nethack" {
  metadata {
    name = "late-nethack"
  }

  spec {
    replicas = 1

    # One RWO volume; allow the brief two-pod overlap on the single node (the
    # host's own NetHack lock files guard concurrent access).
    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 1
        max_unavailable = 0
      }
    }

    selector {
      match_labels = {
        app = "late-nethack"
      }
    }

    template {
      metadata {
        labels = {
          app = "late-nethack"
        }
      }

      spec {
        # Seed the writable playground on the PVC before the host starts. NetHack
        # never mkdir's it, so create save/ (and the append-only record files) and
        # hand the tree to the late user. Idempotent: mkdir -p / touch never
        # clobber existing saves or bones. Runs as root only to chown.
        init_container {
          name  = "nethack-save-seed"
          image = var.NETHACK_IMAGE_TAG
          command = [
            "sh", "-c",
            "mkdir -p ${local.nethack_var_path}/save && touch ${local.nethack_var_path}/record ${local.nethack_var_path}/logfile ${local.nethack_var_path}/xlogfile ${local.nethack_var_path}/perm && chown -R late:late ${local.nethack_var_path}",
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "nethack-save"
            mount_path = local.nethack_var_path
          }
        }

        container {
          image = var.NETHACK_IMAGE_TAG
          name  = "late-nethack"

          port {
            container_port = 2323
            name           = "nethack"
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
              port = "nethack"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "nethack"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "nethack"
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
          # into service-ssh as LATE_NETHACK_SECRET).
          env {
            name = "LATE_NETHACK_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.nethack_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          volume_mount {
            name       = "nethack-save"
            mount_path = local.nethack_var_path
          }
        }

        volume {
          name = "nethack-save"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.nethack_save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "late_nethack_sv" {
  metadata {
    name = "late-nethack-sv"
  }

  spec {
    selector = {
      app = "late-nethack"
    }

    # Cluster-internal only: reached by service-ssh at late-nethack-sv:2323. Not
    # exposed via ingress or the ssh-tcp LoadBalancer.
    port {
      name        = "nethack"
      port        = 2323
      target_port = "nethack"
    }
  }
}
