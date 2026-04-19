# =============================================================================
# Ingress: late.sh (web) + api.late.sh (SSH API with WebSocket)
# =============================================================================

# late.sh → late-web
resource "kubernetes_ingress_v1" "service_web" {
  metadata {
    name = "service-web-ingress"
    annotations = {
      "kubernetes.io/ingress.class"               = "nginx"
      "cert-manager.io/cluster-issuer"            = "letsencrypt-prod"
      "acme.cert-manager.io/http01-edit-in-place" = "true"
    }
  }

  spec {
    tls {
      hosts       = [var.DOMAIN]
      secret_name = "service-web-tls"
    }

    rule {
      host = var.DOMAIN
      http {
        path {
          path      = "/"
          path_type = "Prefix"
          backend {
            service {
              name = kubernetes_service_v1.service_web_sv.metadata[0].name
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

# api.late.sh → late-ssh HTTP API (port 4000)
# WebSocket support for /api/ws/pair browser pairing
resource "kubernetes_ingress_v1" "service_ssh_api" {
  metadata {
    name = "service-ssh-api-ingress"
    annotations = {
      "kubernetes.io/ingress.class"                    = "nginx"
      "nginx.ingress.kubernetes.io/proxy-read-timeout" = "3600"
      "nginx.ingress.kubernetes.io/proxy-send-timeout" = "3600"
      "nginx.ingress.kubernetes.io/proxy-http-version" = "1.1"
      "nginx.ingress.kubernetes.io/upstream-hash-by"   = "$remote_addr"
    }
  }

  spec {
    rule {
      host = "api.${var.DOMAIN}"
      http {
        path {
          path      = "/"
          path_type = "Prefix"
          backend {
            service {
              name = kubernetes_service_v1.service_ssh_sv.metadata[0].name
              port {
                name = "api"
              }
            }
          }
        }
      }
    }
  }
}

# audio.late.sh → Icecast streaming
resource "kubernetes_ingress_v1" "icecast" {
  metadata {
    name = "icecast-ingress"
    annotations = {
      "kubernetes.io/ingress.class"                    = "nginx"
      "cert-manager.io/cluster-issuer"                 = "letsencrypt-prod"
      "acme.cert-manager.io/http01-edit-in-place"      = "true"
      "nginx.ingress.kubernetes.io/proxy-buffering"    = "off"
      "nginx.ingress.kubernetes.io/proxy-read-timeout" = "3600"
      "nginx.ingress.kubernetes.io/proxy-send-timeout" = "3600"
    }
  }

  spec {
    tls {
      hosts       = ["audio.${var.DOMAIN}"]
      secret_name = "icecast-tls"
    }

    rule {
      host = "audio.${var.DOMAIN}"
      http {
        path {
          path      = "/"
          path_type = "Prefix"
          backend {
            service {
              name = kubernetes_service_v1.icecast_sv.metadata[0].name
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
