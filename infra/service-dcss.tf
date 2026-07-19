# =============================================================================
# late-dcss: standalone DCSS door host (game served over SSH)
# =============================================================================
# Runs the real upstream crawl console binary on a PTY per session and serves it
# over SSH. service-ssh reaches it as a network-proxied door (the same model as
# the nethack host). See late-ssh/src/app/door/dcss/CONTEXT.md and the late-dcss
# crate.
#
# Persistence: this pod owns the writable playground. It mounts the `dcss-save`
# PVC (defined in dcss.tf) at the child HOME, so per-player saves under
# $HOME/.crawl survive restarts. crawl creates its own ~/.crawl tree; the
# dcss-save-seed init_container only hands the mount to the `late` user.
#
# replicas MUST stay 1: one RWO volume holds every player's save (see dcss.tf).
# The host pod is always deployed (like service-ssh/nethack/dopewars); the
# door's enable flag only gates the CLIENT (service-ssh's LATE_DCSS_ENABLED).
# Keeping the host unconditional means its image always exists in-cluster, so
# the deploy workflows can read it with a plain `kubectl get` (no bootstrap
# fallback) just like the other images.

resource "kubernetes_deployment_v1" "late_dcss" {
  metadata {
    name = "late-dcss"
  }

  spec {
    replicas = 1

    # Kill-before-create: the old pod fully terminates before the new one starts,
    # so the two never co-mount the RWO volume. On SIGTERM the host SIGHUP-saves
    # its live games (crawl saves-and-exits on hangup) and exits within the grace
    # period below. Costs a few seconds of door downtime per host redeploy, which
    # is fine for a single-replica door.
    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 0
        max_unavailable = 1
      }
    }

    selector {
      match_labels = {
        app = "late-dcss"
      }
    }

    template {
      metadata {
        labels = {
          app = "late-dcss"
        }
      }

      spec {
        # Give the host time to SIGHUP-save in-flight games on SIGTERM before the
        # kubelet SIGKILLs the pod. Must exceed the host's own SHUTDOWN_GRACE
        # (main.rs, ~8s). 30s is the k8s default, pinned here to document the
        # dependency.
        termination_grace_period_seconds = 30

        # Hand the playground HOME on the PVC to the `late` user before the host
        # starts (an empty PVC mount is root-owned). crawl creates its own
        # ~/.crawl tree on first run, so we only fix ownership. Idempotent; runs
        # as root to chown.
        init_container {
          name  = "dcss-save-seed"
          image = var.DCSS_IMAGE_TAG
          command = [
            "sh", "-c",
            "mkdir -p ${local.dcss_var_path} && chown -R late:late ${local.dcss_var_path}",
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "dcss-save"
            mount_path = local.dcss_var_path
          }
        }

        container {
          image = var.DCSS_IMAGE_TAG
          name  = "late-dcss"

          port {
            container_port = 2325
            name           = "dcss"
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
              port = "dcss"
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "dcss"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = "dcss"
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
          # into service-ssh as LATE_DCSS_SECRET).
          env {
            name = "LATE_DCSS_SECRET"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.dcss_identity_secret.metadata[0].name
                key  = "secret"
              }
            }
          }

          # The child HOME on the PVC (crawl writes everything under
          # $HOME/.crawl).
          env {
            name  = "LATE_DCSS_DATA_DIR"
            value = local.dcss_var_path
          }

          volume_mount {
            name       = "dcss-save"
            mount_path = local.dcss_var_path
          }
        }

        volume {
          name = "dcss-save"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.dcss_save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "late_dcss_sv" {
  metadata {
    name = "late-dcss-sv"
  }

  spec {
    selector = {
      app = "late-dcss"
    }

    # Cluster-internal only: reached by service-ssh at late-dcss-sv:2325. Not
    # exposed via ingress or the ssh-tcp LoadBalancer.
    port {
      name        = "dcss"
      port        = 2325
      target_port = "dcss"
    }
  }
}
