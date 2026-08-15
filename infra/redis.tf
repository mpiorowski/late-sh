# =============================================================================
# Valkey (the BSD-licensed Redis fork, protocol-compatible drop-in): the
# psrpc message bus shared by livekit-server and livekit-ingress.
# Once livekit-server has redis configured it routes room lookups and its
# server API RPCs through it, so a redis outage degrades all LiveKit control
# plane work: voice joins, RemoveParticipant kicks, and stream teardown, not
# just OBS ingest. Media already flowing keeps flowing.
# No persistence on purpose: it carries the bus and ingress definitions, both
# re-minted per stream; a restart ends in-flight OBS streams, same failure
# tier as the in-memory stream registry.
# =============================================================================

resource "kubernetes_deployment_v1" "redis" {
  metadata {
    name = "redis"
  }

  spec {
    replicas = 1

    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "redis"
      }
    }

    template {
      metadata {
        labels = {
          app = "redis"
        }
      }

      spec {
        container {
          image = "valkey/valkey:8-alpine"
          name  = "redis"

          args = ["--save", "", "--appendonly", "no"]

          port {
            container_port = 6379
            name           = "redis"
            protocol       = "TCP"
          }

          resources {
            limits = {
              cpu    = "200m"
              memory = "256Mi"
            }
            requests = {
              cpu    = "50m"
              memory = "64Mi"
            }
          }

          readiness_probe {
            tcp_socket {
              port = "redis"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 3
          }

          liveness_probe {
            tcp_socket {
              port = "redis"
            }
            initial_delay_seconds = 10
            period_seconds        = 20
            failure_threshold     = 5
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "redis_sv" {
  metadata {
    name = "redis-sv"
  }

  spec {
    selector = {
      app = "redis"
    }

    port {
      name        = "redis"
      port        = 6379
      target_port = "redis"
    }
  }
}
