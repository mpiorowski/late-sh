# =============================================================================
# Minecraft: Paper server with GriefPrevention
# =============================================================================
# A friends server on the node's public port 25565 (the client default, so
# players type `late.sh` with no DNS work; `late.sh` already resolves to the
# node). The port is a pod hostPort, NOT an ingress-nginx TCP passthrough
# entry: every nginx config reload (cert-manager renewals included) drains
# old workers after 240s and drops long-lived TCP sessions, which for a game
# server means kicking every player. hostPort follows the same RKE2/Canal
# path as livekit's media ports and is IPv4 only, which Minecraft is fine with.
#
# Access model: online-mode (Mojang account verification) plus an enforced
# whitelist. MINECRAFT_WHITELIST / MINECRAFT_OPS seed the lists on every boot;
# runtime additions go through rcon-cli inside the pod (see README.md).
# GriefPrevention gives players self-serve land claims that others cannot
# build in, break, or loot; two gamerules cover the non-player damage
# (creeper/enderman griefing, fire spread).
#
# Ships through deploy_infra.yml like every other manifest. Never touches
# anything else in the cluster.
# =============================================================================

resource "random_password" "minecraft_rcon" {
  length  = 32
  special = false
}

resource "kubernetes_secret_v1" "minecraft" {
  metadata {
    name = "minecraft"
  }

  data = {
    rcon_password = random_password.minecraft_rcon.result
  }
}

# World, plugins, and server jar live here. A played world grows to a few
# GiB; the node disk is 37 GiB shared with everything else, so the claim is
# deliberately small and the server sets a world border (see RCON_CMDS_STARTUP).
resource "kubernetes_persistent_volume_claim_v1" "minecraft_data" {
  metadata {
    name = "minecraft-data"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = "8Gi"
      }
    }

    storage_class_name = "local-path"
  }

  # local-path is WaitForFirstConsumer: the claim binds only once the
  # deployment below schedules a pod. Waiting here deadlocks the apply.
  wait_until_bound = false

  lifecycle {
    prevent_destroy = true
  }

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_deployment_v1" "minecraft" {
  metadata {
    name = "minecraft"
  }

  spec {
    replicas = 1

    # One world, one RWO volume, one hostPort: two pods can never coexist.
    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "minecraft"
      }
    }

    template {
      metadata {
        labels = {
          app = "minecraft"
        }
      }

      spec {
        # The image traps SIGTERM, runs `stop`, and waits for the world save.
        # A big world can take tens of seconds to flush; SIGKILL mid-save
        # corrupts chunks.
        termination_grace_period_seconds = 120

        container {
          name  = "minecraft"
          image = local.minecraft_image

          port {
            container_port = local.minecraft_port
            host_port      = local.minecraft_port
            name           = "minecraft"
            protocol       = "TCP"
          }

          env {
            name  = "EULA"
            value = "TRUE"
          }
          env {
            name  = "TYPE"
            value = "PAPER"
          }
          env {
            name  = "VERSION"
            value = local.minecraft_version
          }
          # JVM heap. The container limit below leaves ~1 GiB for JVM
          # overhead (metaspace, GC, thread stacks, Netty buffers) above it.
          env {
            name  = "MEMORY"
            value = local.minecraft_heap
          }
          # Aikar's GC flags: the standard Paper tuning for G1.
          env {
            name  = "USE_AIKAR_FLAGS"
            value = "TRUE"
          }
          env {
            name  = "ONLINE_MODE"
            value = "TRUE"
          }
          # The image only turns the whitelist on by itself when WHITELIST is
          # non-empty. ENABLE_WHITELIST keeps it on with an empty seed list,
          # otherwise an unset GitHub variable would mean an open server.
          # ENFORCE_WHITELIST kicks a player the moment they are removed.
          env {
            name  = "ENABLE_WHITELIST"
            value = "TRUE"
          }
          env {
            name  = "ENFORCE_WHITELIST"
            value = "TRUE"
          }
          # Seed lists, MERGEd into the on-disk files each boot so names added
          # via rcon survive restarts and a new seed name lands without a wipe.
          # Only set when non-empty: the image resolves every listed name to a
          # UUID at boot, and an empty variable would still trigger that step.
          dynamic "env" {
            for_each = local.minecraft_whitelist == "" ? [] : [local.minecraft_whitelist]
            content {
              name  = "WHITELIST"
              value = env.value
            }
          }
          env {
            name  = "EXISTING_WHITELIST_FILE"
            value = "MERGE"
          }
          dynamic "env" {
            for_each = local.minecraft_ops == "" ? [] : [local.minecraft_ops]
            content {
              name  = "OPS"
              value = env.value
            }
          }
          env {
            name  = "EXISTING_OPS_FILE"
            value = "MERGE"
          }
          # Resolved against Modrinth at boot for the pinned VERSION.
          env {
            name  = "MODRINTH_PROJECTS"
            value = "griefprevention"
          }
          env {
            name  = "MOTD"
            value = "late.sh"
          }
          env {
            name  = "DIFFICULTY"
            value = "normal"
          }
          env {
            name  = "VIEW_DISTANCE"
            value = "10"
          }
          # Re-applied on every boot, so a gamerule an op flips at runtime
          # reverts on restart. Change it here if that is the intent.
          # worldborder caps disk growth (see the PVC comment).
          env {
            name  = "RCON_CMDS_STARTUP"
            value = "gamerule mobGriefing false\ngamerule doFireTick false\nworldborder set 6000"
          }
          env {
            name  = "ENABLE_RCON"
            value = "TRUE"
          }
          env {
            name = "RCON_PASSWORD"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.minecraft.metadata[0].name
                key  = "rcon_password"
              }
            }
          }

          resources {
            limits = {
              cpu    = "2000m"
              memory = "3Gi"
            }
            requests = {
              cpu    = "500m"
              memory = "2560Mi"
            }
          }

          # First boot downloads Paper and the plugin and generates the
          # spawn area; that can run several minutes on this node.
          startup_probe {
            exec {
              command = ["mc-health"]
            }
            period_seconds    = 10
            failure_threshold = 60
          }

          liveness_probe {
            exec {
              command = ["mc-health"]
            }
            period_seconds    = 30
            timeout_seconds   = 5
            failure_threshold = 3
          }

          readiness_probe {
            exec {
              command = ["mc-health"]
            }
            period_seconds  = 10
            timeout_seconds = 5
          }

          volume_mount {
            name       = "data"
            mount_path = "/data"
          }
        }

        volume {
          name = "data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.minecraft_data.metadata[0].name
          }
        }
      }
    }
  }
}
