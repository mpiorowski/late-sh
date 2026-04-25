# =============================================================================
# late-web: Web server (landing page + audio pairing)
# Port: 3000
# =============================================================================

resource "kubernetes_deployment_v1" "service_web" {
  metadata {
    name = "service-web"
  }

  spec {
    replicas = 1

    strategy {
      type = "RollingUpdate"
      rolling_update {
        max_surge       = 1
        max_unavailable = 0
      }
    }

    selector {
      match_labels = {
        app = "service-web"
      }
    }

    template {
      metadata {
        labels = {
          app = "service-web"
        }
      }

      spec {
        container {
          image = var.WEB_IMAGE_TAG
          name  = "service-web"

          port {
            container_port = 3000
            name           = "http"
          }

          resources {
            limits = {
              cpu    = "250m"
              memory = "1Gi"
            }
            requests = {
              cpu    = "100m"
              memory = "256Mi"
            }
          }

          liveness_probe {
            tcp_socket {
              port = "http"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
          }

          readiness_probe {
            http_get {
              path = "/"
              port = "http"
            }
            initial_delay_seconds = 3
            period_seconds        = 5
          }

          env {
            name  = "RUST_LOG"
            value = var.LOG_LEVEL
          }
          env {
            name  = "OTEL_EXPORTER_OTLP_ENDPOINT"
            value = "http://otel-collector.monitoring.svc.cluster.local:4317"
          }
          env {
            name  = "LATE_WEB_PORT"
            value = "3000"
          }
          env {
            name  = "LATE_SSH_INTERNAL_URL"
            value = "http://service-ssh-sv:4000"
          }
          env {
            name  = "LATE_SSH_PUBLIC_URL"
            value = "api.${var.DOMAIN}"
          }
          env {
            name  = "LATE_AUDIO_URL"
            value = "https://audio.${var.DOMAIN}"
          }

          # --- Database (CloudNativePG) ---
          env {
            name  = "LATE_DB_HOST"
            value = "postgres-rw"
          }
          env {
            name  = "LATE_DB_PORT"
            value = "5432"
          }
          env {
            name = "LATE_DB_NAME"
            value_from {
              secret_key_ref {
                name = "postgres-app"
                key  = "dbname"
              }
            }
          }
          env {
            name = "LATE_DB_USER"
            value_from {
              secret_key_ref {
                name = "postgres-app"
                key  = "user"
              }
            }
          }
          env {
            name = "LATE_DB_PASSWORD"
            value_from {
              secret_key_ref {
                name = "postgres-app"
                key  = "password"
              }
            }
          }
          env {
            name  = "LATE_DB_POOL_SIZE"
            value = var.DB_POOL_SIZE
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "service_web_sv" {
  metadata {
    name = "service-web-sv"
  }

  spec {
    selector = {
      app = "service-web"
    }

    port {
      name        = "http"
      port        = 80
      target_port = "http"
    }
  }
}
