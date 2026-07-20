# =============================================================================
# late-usurper: standalone Usurper door host (game served over SSH)
# =============================================================================
# Runs the real upstream USURPER.EXE on a PTY per session and serves it over
# SSH. service-ssh reaches it as a network-proxied door (the same model as the
# nethack/dcss hosts). See late-ssh/src/app/door/usurper/CONTEXT.md and the
# late-usurper crate.
#
# Persistence: this pod owns the writable game tree. It mounts the
# `usurper-save` PVC (defined in usurper.tf) at the game dir, so the one shared
# world survives restarts. The host copies missing seed files from the image at
# boot and sweeps stale lock artifacts; the usurper-save-seed init_container
# only hands the mount to the `late` user.
#
# replicas MUST stay 1: one RWO volume holds the single shared world (see
# usurper.tf). The host pod is always deployed (like the other door hosts); the
# door's enable flag only gates the CLIENT (service-ssh's
# LATE_USURPER_ENABLED). Keeping the host unconditional means its image always
# exists in-cluster, so the deploy workflows can read it with a plain
# `kubectl get` (no bootstrap fallback) just like the other images.

resource "kubernetes_deployment_v1" "late_usurper" {
  metadata {
    name = "late-usurper"
  }

  spec {
    replicas = 1

    # Kill-before-create: the old pod fully terminates before the new one
    # starts, so the two never co-mount the RWO volume. On SIGTERM the host
    # tears live sessions down (SIGHUP then SIGKILL; the game writes its world
    # to disk as it goes) and exits within the grace period below.
    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 0
        max_unavailable = 1
      }
    }

    selector {
      match_labels = {
        app = "late-usurper"
      }
    }

    template {
      metadata {
        labels = {
          app = "late-usurper"
        }
      }

      spec {
        # Give the host time to tear down in-flight sessions on SIGTERM before
        # the kubelet SIGKILLs the pod. Must exceed the host's own
        # SHUTDOWN_GRACE (main.rs, ~8s). 30s is the k8s default, pinned here to
        # document the dependency.
        termination_grace_period_seconds = 30

        # Hand the game tree on the PVC to the `late` user before the host
        # starts (an empty PVC mount is root-owned). The host does the actual
        # seeding itself. Idempotent; runs as root to chown.
        init_container {
          name  = "usurper-save-seed"
          image = var.USURPER_IMAGE_TAG
          command = [
            "sh", "-c",
            "mkdir -p ${local.usurper_var_path} && chown -R late:late ${local.usurper_var_path}",
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "usurper-save"
            mount_path = local.usurper_var_path
          }
        }

        container {
          image = var.USURPER_IMAGE_TAG
          name  = "late-usurper"

          port {
            container_port = 2326
            name           = "usurper"
          }

          resources {
            limits = {
              cpu    = "1000m"
              memory = "512Mi"
            }
            requests = {
              cpu    = "100m"
              memory = "128Mi"
            }
          }

          startup_probe {
            tcp_socket {
              port = "usurper"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "usurper"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "usurper"
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
          # into service-ssh as LATE_USURPER_SECRET).
          env {
            name = "LATE_USURPER_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.usurper_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          # The writable game tree on the PVC (the children's working
          # directory).
          env {
            name  = "LATE_USURPER_GAME_DIR"
            value = local.usurper_var_path
          }

          volume_mount {
            name       = "usurper-save"
            mount_path = local.usurper_var_path
          }
        }

        volume {
          name = "usurper-save"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.usurper_save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "late_usurper_sv" {
  metadata {
    name = "late-usurper-sv"
  }

  spec {
    selector = {
      app = "late-usurper"
    }

    # Cluster-internal only: reached by service-ssh at late-usurper-sv:2326.
    # Not exposed via ingress or the ssh-tcp LoadBalancer.
    port {
      name        = "usurper"
      port        = 2326
      target_port = "usurper"
    }
  }
}
