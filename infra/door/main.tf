# =============================================================================
# door: one standalone door-game host (game served over SSH)
# =============================================================================
# Every door game deploys as the same shape, instantiated per game from the
# doors map in ../doors.tf:
#   - an identity secret: a shared random secret authorizing late-ssh -> this
#     host. The same value is injected into BOTH the service-ssh client
#     (LATE_<NAME>_SECRET, see service-ssh.tf) and this host pod, which each
#     derive the same ed25519 key from it (see the late-<name> crate).
#   - an RWO PVC holding the game's writable state (saves / scores / world),
#     mounted at var_path. prevent_destroy keeps player progress across
#     redeploys.
#   - a single-replica deployment: an idle SSH listener that forks one
#     short-lived child per player. replicas MUST stay 1: one RWO volume holds
#     the shared state, and single-node local-path storage is assumed.
#   - a cluster-internal service: reached by service-ssh at
#     late-<name>-sv:<port>, never via ingress or the ssh-tcp LoadBalancer
#     (dcss's extra crawl publisher port + ingress live in doors.tf).
#
# The host pod is always deployed (like service-ssh/web); whether the door
# shows up in the TUI is a late-ssh config.rs profile literal.
#
# The image is set by terraform only on first create (a door bootstrap via
# deploy_service.yml); afterwards deploys go through `kubectl set image` and
# the lifecycle block below keeps applies from touching the running image.

variable "name" {
  description = "Door name (nethack, dcss, ...). Everything derives from it: late-<name> deployment, late-<name>-sv service, <name>-save PVC, <name>-identity-secret, LATE_<NAME>_SECRET env."
  type        = string
}

variable "port" {
  description = "The host's SSH listener port."
  type        = number
}

variable "image" {
  description = "Full image ref for the late-<name> host. Only applied on first create; see the lifecycle block."
  type        = string
}

variable "var_path" {
  description = "Mount path of the writable game state PVC inside the pod."
  type        = string
}

variable "pvc_size" {
  description = "Requested size of the game state PVC."
  type        = string
}

variable "seed_container_name" {
  description = "Name of the init container that prepares the PVC mount."
  type        = string
}

variable "seed_command" {
  description = "sh -c command the seed init container runs before the host starts (runs as root; at minimum it chowns the mount to late:late)."
  type        = string
}

variable "extra_env" {
  description = "Additional plain env vars for the host container (data dir / score file paths and the like)."
  type        = map(string)
}

variable "extra_ports" {
  description = "Additional container/service ports beyond the SSH listener (dcss's crawl publisher)."
  type = list(object({
    name = string
    port = number
  }))
}

variable "log_level" {
  description = "Rust log level (RUST_LOG)."
  type        = string
}

variable "pull_secret_name" {
  description = "Name of the image pull secret for ghcr."
  type        = string
}

variable "cpu_request" {
  type = string
}

variable "memory_request" {
  type = string
}

variable "cpu_limit" {
  type = string
}

variable "memory_limit" {
  type = string
}

locals {
  app_name        = "late-${var.name}"
  secret_env_name = "LATE_${upper(var.name)}_SECRET"
}

resource "random_password" "identity" {
  length  = 64
  special = false
}

resource "kubernetes_secret_v1" "identity" {
  metadata {
    name = "${var.name}-identity-secret"
  }

  data = {
    secret = random_password.identity.result
  }

  type = "Opaque"
}

resource "kubernetes_persistent_volume_claim_v1" "save" {
  metadata {
    name = "${var.name}-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = var.pvc_size
      }
    }

    storage_class_name = "local-path"
  }

  wait_until_bound = false

  lifecycle {
    prevent_destroy = true
  }
}

resource "kubernetes_deployment_v1" "this" {
  metadata {
    name = local.app_name
  }

  spec {
    replicas = 1

    # Kill-before-create: the old pod fully terminates before the new one
    # starts, so the two never co-mount the RWO volume. Hosts that hold live
    # games SIGHUP-save them on SIGTERM and exit within the grace period
    # below. Costs a few seconds of door downtime per redeploy, which is fine
    # for a single-replica door.
    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 0
        max_unavailable = 1
      }
    }

    selector {
      match_labels = {
        app = local.app_name
      }
    }

    template {
      metadata {
        labels = {
          app = local.app_name
        }
      }

      spec {
        # Give the host time to SIGHUP-save in-flight games on SIGTERM before
        # the kubelet SIGKILLs the pod. Must exceed the host's own
        # SHUTDOWN_GRACE (main.rs, ~8s). 30s is the k8s default, pinned here
        # to document the dependency.
        termination_grace_period_seconds = 30

        # Prepare the writable state on the PVC before the host starts (an
        # empty PVC mount is root-owned). Idempotent; runs as root to chown.
        # The command comes from the doors map: most games only need
        # mkdir + chown, nethack also seeds save/ and sweeps stale locks.
        init_container {
          name  = var.seed_container_name
          image = var.image
          command = [
            "sh", "-c",
            var.seed_command,
          ]

          security_context {
            run_as_user = 0
          }

          volume_mount {
            name       = "${var.name}-save"
            mount_path = var.var_path
          }
        }

        container {
          image = var.image
          name  = local.app_name

          port {
            container_port = var.port
            name           = var.name
          }

          dynamic "port" {
            for_each = var.extra_ports
            content {
              container_port = port.value.port
              name           = port.value.name
            }
          }

          resources {
            limits = {
              cpu    = var.cpu_limit
              memory = var.memory_limit
            }
            requests = {
              cpu    = var.cpu_request
              memory = var.memory_request
            }
          }

          startup_probe {
            tcp_socket {
              port = var.name
            }
            initial_delay_seconds = 5
            period_seconds        = 5
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = var.name
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          readiness_probe {
            tcp_socket {
              port = var.name
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 6
          }

          env {
            name  = "RUST_LOG"
            value = var.log_level
          }

          # Shared secret authorizing late-ssh -> this host (same value
          # injected into service-ssh as LATE_<NAME>_SECRET).
          env {
            name = local.secret_env_name
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.identity.metadata[0].name
                key  = "secret"
              }
            }
          }

          dynamic "env" {
            for_each = var.extra_env
            content {
              name  = env.key
              value = env.value
            }
          }

          volume_mount {
            name       = "${var.name}-save"
            mount_path = var.var_path
          }
        }

        volume {
          name = "${var.name}-save"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.save.metadata[0].name
          }
        }

        image_pull_secrets {
          name = var.pull_secret_name
        }
      }
    }
  }

  # Images are deployed with `kubectl set image` (deploy_service.yml), never
  # by terraform applies, so a full apply must not roll a component back to
  # whatever tag it was created with.
  lifecycle {
    ignore_changes = [
      spec[0].template[0].spec[0].container[0].image,
      spec[0].template[0].spec[0].init_container[0].image,
    ]
  }
}

resource "kubernetes_service_v1" "this" {
  metadata {
    name = "${local.app_name}-sv"
  }

  spec {
    selector = {
      app = local.app_name
    }

    port {
      name        = var.name
      port        = var.port
      target_port = var.name
    }

    dynamic "port" {
      for_each = var.extra_ports
      content {
        name        = port.value.name
        port        = port.value.port
        target_port = port.value.name
      }
    }
  }
}

output "identity_secret_name" {
  description = "Name of the door's identity secret, injected into service-ssh."
  value       = kubernetes_secret_v1.identity.metadata[0].name
}

output "service_name" {
  description = "Name of the door's cluster-internal service."
  value       = kubernetes_service_v1.this.metadata[0].name
}
