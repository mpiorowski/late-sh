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
          image = local.image_tags["web"]
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
          # Per-pod telemetry identity: without service.instance.id every pod
          # exports the same otel series and they clobber each other on scrape
          # (fatal once service-web runs multiple replicas). $(POD_NAME) is the
          # downward-API pod name; the SDK's env resource detector reads
          # OTEL_RESOURCE_ATTRIBUTES, the collector turns it into a metric label.
          env {
            name = "POD_NAME"
            value_from {
              field_ref {
                field_path = "metadata.name"
              }
            }
          }
          env {
            name  = "OTEL_RESOURCE_ATTRIBUTES"
            value = "service.instance.id=$(POD_NAME)"
          }
          # Selects the config.rs profile; every non-secret value lives there.
          env {
            name  = "LATE_ENV"
            value = "prod"
          }

          # --- Database (CloudNativePG operator-generated credentials) ---
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
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }
      }
    }
  }

  # Images are deployed with `kubectl set image` (deploy_service.yml), never
  # by terraform applies, so a full apply must not roll the service back to
  # whatever tag it was created with.
  lifecycle {
    ignore_changes = [
      spec[0].template[0].spec[0].container[0].image,
    ]
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
