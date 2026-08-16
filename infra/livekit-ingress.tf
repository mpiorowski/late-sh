# =============================================================================
# LiveKit Ingress: WHIP ingest for OBS streams (`/golive obs`)
#
# OBS pushes WebRTC (h264+opus) to https://whip.<domain>/w with a per-stream
# bearer token; this service forwards it into the stream room's LiveKit
# channel. Transcoding is disabled by the app at CreateIngress time, so this
# stays a packet forwarder, not an encoder. It talks to livekit-server over
# the shared redis bus (redis.tf).
# =============================================================================

locals {
  livekit_ingress_config = yamlencode({
    api_key    = local.livekit_api_key
    api_secret = random_password.livekit_api_secret.result
    ws_url     = "ws://livekit-sv"

    redis = {
      address = "redis-sv:6379"
    }

    # WHIP signaling (HTTP) behind the nginx ingress below.
    whip_port       = local.livekit_ingress_whip_port
    http_relay_port = local.livekit_ingress_whip_port + 1

    # WHIP media: one muxed ICE/UDP port on the node (host network, same
    # pattern as livekit itself).
    rtc_config = {
      udp_port        = local.livekit_ingress_whip_port + 2
      use_external_ip = local.livekit_rtc_use_external_ip
    }

    # The RTMP listener is bound (upstream default) but inert: the app only
    # ever creates WHIP ingresses, so no stream key routes through RTMP.
    # Keep 1935 closed at the node firewall.
    rtmp_port = 1935

    logging = {
      level = local.livekit_log_level
    }
  })
}

resource "kubernetes_secret_v1" "livekit_ingress" {
  metadata {
    name = "livekit-ingress"
  }

  data = {
    "config.yaml" = local.livekit_ingress_config
  }
}

resource "kubernetes_deployment_v1" "livekit_ingress" {
  metadata {
    name = "livekit-ingress"
  }

  spec {
    replicas = 1

    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "livekit-ingress"
      }
    }

    template {
      metadata {
        labels = {
          app = "livekit-ingress"
        }
        annotations = {
          config_hash = sha256(local.livekit_ingress_config)
        }
      }

      spec {
        host_network                     = true
        dns_policy                       = "ClusterFirstWithHostNet"
        termination_grace_period_seconds = 30

        container {
          image = local.livekit_ingress_image
          name  = "livekit-ingress"

          env {
            name  = "INGRESS_CONFIG_FILE"
            value = "/etc/livekit-ingress/config.yaml"
          }

          port {
            container_port = local.livekit_ingress_whip_port
            host_port      = local.livekit_ingress_whip_port
            name           = "whip"
            protocol       = "TCP"
          }

          port {
            container_port = local.livekit_ingress_whip_port + 2
            host_port      = local.livekit_ingress_whip_port + 2
            name           = "whip-udp"
            protocol       = "UDP"
          }

          resources {
            limits = {
              cpu    = "1000m"
              memory = "1Gi"
            }
            requests = {
              cpu    = "100m"
              memory = "256Mi"
            }
          }

          readiness_probe {
            tcp_socket {
              port = "whip"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 6
          }

          liveness_probe {
            tcp_socket {
              port = "whip"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          volume_mount {
            name       = "config"
            mount_path = "/etc/livekit-ingress"
            read_only  = true
          }
        }

        volume {
          name = "config"

          secret {
            secret_name = kubernetes_secret_v1.livekit_ingress.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "livekit_ingress" {
  metadata {
    name = "livekit-ingress-sv"
  }

  spec {
    selector = {
      app = "livekit-ingress"
    }

    port {
      name        = "whip"
      port        = 80
      target_port = "whip"
    }
  }
}

resource "kubernetes_ingress_v1" "livekit_whip" {
  metadata {
    name = "livekit-whip-ingress"
    annotations = {
      "kubernetes.io/ingress.class"                    = "nginx"
      "cert-manager.io/cluster-issuer"                 = "letsencrypt-prod"
      "acme.cert-manager.io/http01-edit-in-place"      = "true"
      "nginx.ingress.kubernetes.io/proxy-read-timeout" = "3600"
      "nginx.ingress.kubernetes.io/proxy-send-timeout" = "3600"
      "nginx.ingress.kubernetes.io/proxy-http-version" = "1.1"
    }
  }

  spec {
    tls {
      hosts       = [local.livekit_whip_host]
      secret_name = "livekit-whip-tls"
    }

    rule {
      host = local.livekit_whip_host
      http {
        path {
          path      = "/"
          path_type = "Prefix"
          backend {
            service {
              name = kubernetes_service_v1.livekit_ingress.metadata[0].name
              port {
                name = "whip"
              }
            }
          }
        }
      }
    }
  }
}
