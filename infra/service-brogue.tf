# =============================================================================
# late-brogue: standalone Brogue door host (game served over SSH)
# =============================================================================
# Runs the real upstream Brogue CE curses binary on a PTY per session and
# serves it over SSH. service-ssh reaches it as a network-proxied door (the
# same model as the dcss host). See late-ssh/src/app/door/brogue/CONTEXT.md and
# the late-brogue crate.
#
# Persistence: this pod owns the writable playground. It mounts the
# `brogue-save` PVC (defined in brogue.tf) at the playground root, so the
# per-player save directories under players/ survive restarts. The host creates
# each player directory on demand; the brogue-save-seed init_container only
# hands the mount to the `late` user.
#
# replicas MUST stay 1: one RWO volume holds every player's save (see
# brogue.tf). The host pod is always deployed (like service-ssh/dcss); the
# door's enable flag only gates the CLIENT (service-ssh's LATE_BROGUE_ENABLED).
# Keeping the host unconditional means its image always exists in-cluster, so
# the deploy workflows can read it with a plain `kubectl get` (no bootstrap
# fallback) just like the other images.

resource "kubernetes_deployment_v1" "late_brogue" {
  metadata {
    name = "late-brogue"
  }

  spec {
    replicas = 1

    # Kill-before-create: the old pod fully terminates before the new one
    # starts, so the two never co-mount the RWO volume. On SIGTERM the host
    # SIGHUP-saves its live games (our brogue build saves-and-exits on hangup,
    # see scripts/brogue_hangup_save.patch) and exits within the grace period
    # below. Costs a few seconds of door downtime per host redeploy, which is
    # fine for a single-replica door.
    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 0
        max_unavailable = 1
      }
    }

    selector {
      match_labels = {
        app = "late-brogue"
      }
    }

    template {
      metadata {
        labels = {
          app = "late-brogue"
        }
      }

      spec {
        # Give the host time to SIGHUP-save in-flight games on SIGTERM before
        # the kubelet SIGKILLs the pod. Must exceed the host's own
        # SHUTDOWN_GRACE (main.rs, ~8s). 30s is the k8s default, pinned here to
        # document the dependency.
        termination_grace_period_seconds = 30

        # Hand the playground root on the PVC to the `late` user before the
        # host starts (an empty PVC mount is root-owned). The host creates the
        # per-player directories itself, so we only fix ownership. Idempotent;
        # runs as root to chown.
        init_container {
          name  = "brogue-save-seed"
          image = var.BROGUE_IMAGE_TAG
          command = [
            "sh", "-c",
            "mkdir -p ${local.brogue_var_path} && chown -R late:late ${local.brogue_var_path}",
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "brogue-save"
            mount_path = local.brogue_var_path
          }
        }

        container {
          image = var.BROGUE_IMAGE_TAG
          name  = "late-brogue"

          port {
            container_port = 2327
            name           = "brogue"
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
              port = "brogue"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "brogue"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "brogue"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 6
          }

          env {
            name  = "RUST_LOG"
            value = var.LOG_LEVEL
          }

          # Shared secret authorizing late-ssh -> this host (same value
          # injected into service-ssh as LATE_BROGUE_SECRET).
          env {
            name = "LATE_BROGUE_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.brogue_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          # The playground root on the PVC (each child runs in
          # players/<playname> under it).
          env {
            name  = "LATE_BROGUE_DATA_DIR"
            value = local.brogue_var_path
          }

          volume_mount {
            name       = "brogue-save"
            mount_path = local.brogue_var_path
          }
        }

        volume {
          name = "brogue-save"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.brogue_save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "late_brogue_sv" {
  metadata {
    name = "late-brogue-sv"
  }

  spec {
    selector = {
      app = "late-brogue"
    }

    # Cluster-internal only: reached by service-ssh at late-brogue-sv:2327. Not
    # exposed via ingress or the ssh-tcp LoadBalancer.
    port {
      name        = "brogue"
      port        = 2327
      target_port = "brogue"
    }
  }
}
