# =============================================================================
# Vintage Story: the official dedicated server, on its own arm64 node
# =============================================================================
# A friends server on agent-2's public port 42420, the client default, over
# TCP and UDP (the game uses both). agent-2 is the game-server node: a Hetzner
# CAX21 (arm64) labelled and tainted `role=games` (defaults.tf, node
# placement), so nothing else lands there and this pod has the node to itself.
# Players reach it through a manual DNS record, `vs.late.sh` A agent-2
# (infra/README.md, Vintage Story), because `*.late.sh` is Cloudflare-proxied
# and points at server-1. The ports are pod hostPorts for the same reason as
# Minecraft's: an ingress-nginx TCP entry drops every player on each reload.
#
# The image is ours (docker/vintagestory/Dockerfile): Anego's linux-x64
# server tarball plus Anego's arm64 build of its launcher and native
# libraries. The server traps SIGTERM and saves the world before exiting 0.
#
# Access model: account verification against Anego's auth server plus the
# server's own whitelist. Players are added from the console over stdin
# (`kubectl attach`, see late-ssh/src/app/door/vintagestory/CONTEXT.md);
# the whitelist lives in Playerdata/ on the volume. Land claims are built into
# the game and run on its role defaults.
#
# The Games hub card (late-ssh/src/app/door/vintagestory/ui.rs) quotes the
# version, address, and the settings below to players; its ui_test.rs reads
# this file, defaults.tf, and the Dockerfile and fails when they drift.
#
# Ships through deploy_infra.yml like every other manifest.
# =============================================================================

locals {
  # Passed as --withconfig on every boot, so these keys always hold on start
  # and the server writes them into serverconfig.json. A runtime
  # `/serverconfig` change to one of them reverts on restart. Every other key
  # keeps the game default, or whatever an admin set at runtime.
  vintagestory_config = {
    ServerName        = "late.sh"
    WhitelistMode     = "On"
    VerifyPlayerAuth  = true
    AdvertiseServer   = false
    Upnp              = false
    MaxClients        = 16
    AllowPvP          = true
    AllowFireSpread   = true
    PassTimeWhenEmpty = false
  }
}

# World, player data, whitelist, config, logs, and the game's own backups.
# local-path does not enforce the size, so this is a label, not a cap: the
# node disk (37 GiB, nothing else on it) is the real limit.
resource "kubernetes_persistent_volume_claim_v1" "vintagestory_data" {
  metadata {
    name = "vintagestory-data"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = "10Gi"
      }
    }

    storage_class_name = "local-path"
  }

  # local-path is WaitForFirstConsumer: the claim binds only once the
  # deployment below schedules a pod. Waiting here deadlocks the apply.
  wait_until_bound = false

  # The volume's reclaim policy is Delete: destroying this claim erases the
  # world from agent-2's disk.
  lifecycle {
    prevent_destroy = true
  }

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_deployment_v1" "vintagestory" {
  metadata {
    name = "vintagestory"
  }

  spec {
    replicas = 1

    # One world, one RWO volume, one hostPort: two pods can never coexist.
    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "vintagestory"
      }
    }

    template {
      metadata {
        labels = {
          app = "vintagestory"
        }
      }

      spec {
        # SIGTERM saves the world and exits within seconds on a small world;
        # the grace covers a big one. SIGKILL mid-save loses the unsaved
        # chunks.
        termination_grace_period_seconds = 120

        # Game-server workload: runs on agent-2 (defaults.tf, node placement).
        # The arch selector is explicit because the image is arm64 only.
        node_selector = {
          (local.games_node_label_key) = local.games_node_label_value
          "kubernetes.io/arch"         = "arm64"
        }

        toleration {
          key      = local.games_node_label_key
          operator = "Equal"
          value    = local.games_node_label_value
          effect   = "NoSchedule"
        }

        security_context {
          run_as_non_root = true
          run_as_user     = 1654
          run_as_group    = 1654
        }

        container {
          name  = "vintagestory"
          image = local.vintagestory_image

          args = [
            "--dataPath",
            "/data",
            "--withconfig=${jsonencode(local.vintagestory_config)}",
          ]

          # The server console reads stdin: `kubectl attach -i` sends it
          # commands. stdin_once stays false so the pipe outlives each attach.
          stdin      = true
          stdin_once = false
          tty        = false

          port {
            container_port = local.vintagestory_port
            host_port      = local.vintagestory_port
            name           = "vs-tcp"
            protocol       = "TCP"
          }

          port {
            container_port = local.vintagestory_port
            host_port      = local.vintagestory_port
            name           = "vs-udp"
            protocol       = "UDP"
          }

          # Measured on agent-2: about 1.2 GiB with the spawn area loaded and
          # nobody on. The official estimate adds about 300 MiB per player.
          resources {
            limits = {
              cpu    = "3000m"
              memory = "3Gi"
            }
            requests = {
              cpu    = "500m"
              memory = "1536Mi"
            }
          }

          # First boot generates the world (about 40 s on agent-2); a version
          # upgrade converts it, which can take longer.
          startup_probe {
            tcp_socket {
              port = local.vintagestory_port
            }
            period_seconds    = 10
            failure_threshold = 60
          }

          liveness_probe {
            tcp_socket {
              port = local.vintagestory_port
            }
            period_seconds    = 30
            timeout_seconds   = 5
            failure_threshold = 3
          }

          readiness_probe {
            tcp_socket {
              port = local.vintagestory_port
            }
            period_seconds  = 10
            timeout_seconds = 5
          }

          volume_mount {
            name       = "data"
            mount_path = "/data"
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }

        volume {
          name = "data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.vintagestory_data.metadata[0].name
          }
        }
      }
    }
  }
}
