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
# The host pod is always deployed (like service-ssh/nethack/dopewars); whether
# the door shows up in the TUI is a late-ssh config.rs profile literal.
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

          # Read-only HTTP publishing of the shared crawl logs for the public
          # DCSS tooling (see the ingress at the bottom of this file).
          port {
            container_port = 2329
            name           = "crawl"
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

          # Port for the read-only crawl-file publisher. Stated explicitly so
          # this manifest's container port, service port, and ingress all read
          # from one place rather than from the host's default.
          env {
            name  = "LATE_DCSS_PUBLISH_PORT"
            value = "2329"
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

    # The game itself is cluster-internal: reached by service-ssh at
    # late-dcss-sv:2325, never through ingress or the ssh-tcp LoadBalancer.
    port {
      name        = "dcss"
      port        = 2325
      target_port = "dcss"
    }

    # The crawl-file publisher, fronted by the ingress below.
    port {
      name        = "crawl"
      port        = 2329
      target_port = "crawl"
    }
  }
}

# =============================================================================
# late.sh/crawl → the read-only crawl server files (logfile, milestones, morgue)
# =============================================================================
# Public DCSS tooling (dcss-stats.com, Sequell) ingests a server by fetching its
# logfile/milestones over HTTP and linking morgue dumps per game. The late-dcss
# host serves those natively under /crawl/... (late-dcss/src/publish.rs), so no
# rewrite annotation is needed here.
#
# Path prefix rather than a subdomain, deliberately: ingress-nginx merges every
# rule for a host into one server block and matches longest-prefix-first, so
# /crawl out-ranks late-web's "/" catch-all in ingress.tf while the existing DNS
# record and TLS certificate keep working untouched. That is also why this
# resource carries no tls block and no cert-manager annotations: the certificate
# for local.domain belongs to service-web-ingress, and a second issuer request
# for the same host would fight it.
resource "kubernetes_ingress_v1" "late_dcss_crawl" {
  metadata {
    name = "late-dcss-crawl-ingress"
    annotations = {
      "kubernetes.io/ingress.class" = "nginx"
    }
  }

  spec {
    rule {
      host = local.domain
      http {
        path {
          path      = "/crawl"
          path_type = "Prefix"
          backend {
            service {
              name = kubernetes_service_v1.late_dcss_sv.metadata[0].name
              port {
                name = "crawl"
              }
            }
          }
        }
      }
    }
  }
}
