# =============================================================================
# LiveKit: voice-room SFU / RTC service
# =============================================================================

locals {
  livekit_host = "${var.LIVEKIT_SUBDOMAIN}.${var.DOMAIN}"
  livekit_url  = "wss://${local.livekit_host}"

  livekit_config = yamlencode({
    port = 7880

    rtc = {
      tcp_port        = var.LIVEKIT_RTC_TCP_PORT
      udp_port        = var.LIVEKIT_RTC_UDP_PORT
      use_external_ip = var.LIVEKIT_RTC_USE_EXTERNAL_IP
    }

    turn = {
      enabled   = var.LIVEKIT_TURN_ENABLED
      domain    = local.livekit_host
      udp_port  = var.LIVEKIT_TURN_UDP_PORT
      tls_port  = var.LIVEKIT_TURN_TLS_PORT
      cert_file = "/etc/livekit-tls/tls.crt"
      key_file  = "/etc/livekit-tls/tls.key"
    }

    keys = {
      (var.LIVEKIT_API_KEY) = random_password.livekit_api_secret.result
    }

    room = {
      auto_create       = true
      empty_timeout     = 300
      departure_timeout = 20
      enabled_codecs = [
        {
          mime = "audio/opus"
        }
      ]
    }

    logging = {
      level = var.LIVEKIT_LOG_LEVEL
    }
  })
}

resource "random_password" "livekit_api_secret" {
  length  = 48
  special = false
}

resource "kubernetes_secret_v1" "livekit" {
  metadata {
    name = "livekit"
  }

  data = {
    api_key      = var.LIVEKIT_API_KEY
    api_secret   = random_password.livekit_api_secret.result
    "config.yml" = local.livekit_config
  }
}

resource "kubernetes_deployment_v1" "livekit" {
  metadata {
    name = "livekit"
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
        app = "livekit"
      }
    }

    template {
      metadata {
        labels = {
          app = "livekit"
        }
        annotations = {
          config_hash = sha256(local.livekit_config)
        }
      }

      spec {
        host_network                     = true
        dns_policy                       = "ClusterFirstWithHostNet"
        termination_grace_period_seconds = 30

        container {
          image = var.LIVEKIT_IMAGE
          name  = "livekit"

          command = ["livekit-server"]
          args    = ["--config", "/etc/livekit/config.yml"]

          port {
            container_port = 7880
            name           = "http"
            protocol       = "TCP"
          }

          port {
            container_port = var.LIVEKIT_RTC_TCP_PORT
            host_port      = var.LIVEKIT_RTC_TCP_PORT
            name           = "rtc-tcp"
            protocol       = "TCP"
          }

          port {
            container_port = var.LIVEKIT_RTC_UDP_PORT
            host_port      = var.LIVEKIT_RTC_UDP_PORT
            name           = "rtc-udp"
            protocol       = "UDP"
          }

          port {
            container_port = var.LIVEKIT_TURN_UDP_PORT
            host_port      = var.LIVEKIT_TURN_UDP_PORT
            name           = "turn-udp"
            protocol       = "UDP"
          }

          port {
            container_port = var.LIVEKIT_TURN_TLS_PORT
            host_port      = var.LIVEKIT_TURN_TLS_PORT
            name           = "turn-tls"
            protocol       = "TCP"
          }

          resources {
            limits = {
              cpu    = "1000m"
              memory = "1Gi"
            }
            requests = {
              cpu    = "250m"
              memory = "256Mi"
            }
          }

          readiness_probe {
            tcp_socket {
              port = "http"
            }
            initial_delay_seconds = 5
            period_seconds        = 10
            failure_threshold     = 6
          }

          liveness_probe {
            tcp_socket {
              port = "http"
            }
            initial_delay_seconds = 15
            period_seconds        = 20
            failure_threshold     = 5
          }

          volume_mount {
            name       = "config"
            mount_path = "/etc/livekit"
            read_only  = true
          }

          volume_mount {
            name       = "tls"
            mount_path = "/etc/livekit-tls"
            read_only  = true
          }
        }

        volume {
          name = "config"

          secret {
            secret_name = kubernetes_secret_v1.livekit.metadata[0].name
          }
        }

        volume {
          name = "tls"

          secret {
            secret_name = "livekit-tls"
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "livekit" {
  metadata {
    name = "livekit-sv"
  }

  spec {
    selector = {
      app = "livekit"
    }

    port {
      name        = "http"
      port        = 80
      target_port = "http"
    }
  }
}

resource "kubernetes_ingress_v1" "livekit" {
  metadata {
    name = "livekit-ingress"
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
      hosts       = [local.livekit_host]
      secret_name = "livekit-tls"
    }

    rule {
      host = local.livekit_host
      http {
        path {
          path      = "/"
          path_type = "Prefix"
          backend {
            service {
              name = kubernetes_service_v1.livekit.metadata[0].name
              port {
                name = "http"
              }
            }
          }
        }
      }
    }
  }
}
