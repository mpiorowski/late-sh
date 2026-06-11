# =============================================================================
# Liquidsoap: Playlist manager + audio encoder
# Streams to Icecast, accepts vibe switches via telnet (port 1234)
# =============================================================================

resource "kubernetes_config_map_v1" "liquidsoap_config" {
  metadata {
    name = "liquidsoap-config"
  }

  data = {
    "radio.liq" = replace(
      file("${path.module}/liquidsoap/radio.liq"),
      "hackme", random_password.icecast_source.result
    )
  }
}

resource "kubernetes_config_map_v1" "liquidsoap_playlists" {
  metadata {
    name = "liquidsoap-playlists"
  }

  data = {
    "lofi.m3u"    = file("${path.module}/liquidsoap/lofi.m3u")
    "classic.m3u" = file("${path.module}/liquidsoap/classic.m3u")
    "ambient.m3u" = file("${path.module}/liquidsoap/ambient.m3u")
  }
}

# PVC for music files — synced from R2 during deploy (sync_music job in deploy_infra.yml)
resource "kubernetes_persistent_volume_claim_v1" "music_data" {
  metadata {
    name = "music-data"
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

  wait_until_bound = false

  lifecycle {
    prevent_destroy = true
  }

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_deployment_v1" "liquidsoap" {
  metadata {
    name = "liquidsoap"
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = "liquidsoap"
      }
    }

    template {
      metadata {
        labels = {
          app = "liquidsoap"
        }
        annotations = {
          config_hash   = sha256(join("", values(kubernetes_config_map_v1.liquidsoap_config.data)))
          playlist_hash = sha256(join("", values(kubernetes_config_map_v1.liquidsoap_playlists.data)))
        }
      }

      spec {
        container {
          name    = "liquidsoap"
          image   = "savonet/liquidsoap:v2.4.0"
          command = ["liquidsoap", "/etc/liquidsoap/radio.liq"]

          port {
            container_port = 1234
            name           = "telnet"
          }

          resources {
            limits = {
              cpu    = "500m"
              memory = "1Gi"
            }
            requests = {
              cpu    = "100m"
              memory = "256Mi"
            }
          }

          volume_mount {
            name       = "config"
            mount_path = "/etc/liquidsoap/radio.liq"
            sub_path   = "radio.liq"
            read_only  = true
          }

          volume_mount {
            name       = "playlists"
            mount_path = "/etc/liquidsoap/lofi.m3u"
            sub_path   = "lofi.m3u"
            read_only  = true
          }

          volume_mount {
            name       = "playlists"
            mount_path = "/etc/liquidsoap/classic.m3u"
            sub_path   = "classic.m3u"
            read_only  = true
          }

          volume_mount {
            name       = "playlists"
            mount_path = "/etc/liquidsoap/ambient.m3u"
            sub_path   = "ambient.m3u"
            read_only  = true
          }

          volume_mount {
            name       = "music"
            mount_path = "/music"
          }
        }

        volume {
          name = "config"

          config_map {
            name = kubernetes_config_map_v1.liquidsoap_config.metadata[0].name
          }
        }

        volume {
          name = "playlists"

          config_map {
            name = kubernetes_config_map_v1.liquidsoap_playlists.metadata[0].name
          }
        }

        volume {
          name = "music"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.music_data.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "liquidsoap_sv" {
  metadata {
    name = "liquidsoap-sv"
  }

  spec {
    selector = {
      app = "liquidsoap"
    }

    port {
      name        = "telnet"
      port        = 1234
      target_port = "telnet"
    }
  }
}
