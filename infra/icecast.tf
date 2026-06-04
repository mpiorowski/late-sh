# =============================================================================
# Icecast: Audio streaming server
# =============================================================================

resource "kubernetes_config_map_v1" "icecast_config" {
  metadata {
    name = "icecast-config"
  }

  data = {
    "icecast.xml" = replace(
      replace(
        replace(
          file("${path.module}/icecast/icecast.xml"),
          "hackme", random_password.icecast_source.result
        ),
        "changeme</relay-password>", "${random_password.icecast_relay.result}</relay-password>"
      ),
      "changeme</admin-password>", "${random_password.icecast_admin.result}</admin-password>"
    )
  }
}

resource "kubernetes_deployment_v1" "icecast" {
  metadata {
    name = "icecast"
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = "icecast"
      }
    }

    template {
      metadata {
        labels = {
          app = "icecast"
        }
        annotations = {
          config_hash = sha256(join("", values(kubernetes_config_map_v1.icecast_config.data)))
        }
      }

      spec {
        container {
          name  = "icecast"
          image = "libretime/icecast:2.4.4"

          port {
            container_port = 8000
            name           = "http"
          }

          resources {
            limits = {
              cpu    = "500m"
              memory = "512Mi"
            }
            requests = {
              cpu    = "100m"
              memory = "128Mi"
            }
          }

          liveness_probe {
            tcp_socket {
              port = "http"
            }
            initial_delay_seconds = 10
            period_seconds        = 20
          }

          readiness_probe {
            tcp_socket {
              port = "http"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
          }

          volume_mount {
            name       = "config"
            mount_path = "/etc/icecast.xml"
            sub_path   = "icecast.xml"
            read_only  = true
          }
        }

        volume {
          name = "config"

          config_map {
            name = kubernetes_config_map_v1.icecast_config.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "icecast_sv" {
  metadata {
    name = "icecast-sv"
  }

  spec {
    selector = {
      app = "icecast"
    }

    port {
      name        = "http"
      port        = 8000
      target_port = "http"
    }
  }
}
