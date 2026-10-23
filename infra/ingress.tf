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
      hosts       = [local.domain]
      secret_name = "service-web-tls"
    }

    rule {
      host = local.domain
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
# WebSocket support for /api/ws/pair CLI/webview pairing
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
      host = "api.${local.domain}"
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
      hosts       = ["audio.${local.domain}"]
      secret_name = "icecast-tls"
    }

    rule {
      host = "audio.${local.domain}"
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

# =============================================================================
# RKE2 ingress-nginx controller: HTTP(S) only
# =============================================================================
# No TCP services: public SSH and IRC go through ssh-edge (ssh-tcp.tf), so an
# nginx reload never touches them. Native host networking keeps the client
# socket address on both IPv4 and IPv6; the former IPv6 TCP proxy lost it.

moved {
  from = kubernetes_manifest.nginx_tcp_config
  to   = kubernetes_manifest.ingress_nginx_config
}

resource "kubernetes_manifest" "ingress_nginx_config" {
  manifest = {
    apiVersion = "helm.cattle.io/v1"
    kind       = "HelmChartConfig"
    metadata = {
      name      = "rke2-ingress-nginx"
      namespace = "kube-system"
    }
    spec = {
      valuesContent = yamlencode({
        controller = {
          # During the first maintenance cutover, delete the old IPv6 proxy
          # before this pod starts: both need the node's public IPv6 :80/:443.
          # hostPort through Canal only exposed the IPv4 side.
          hostNetwork = true
          dnsPolicy   = "ClusterFirstWithHostNet"
          hostPort = {
            enabled = false
          }
          config = {
            # The ingress is directly internet-facing. Ignore client-supplied
            # forwarding headers and forward its observed socket address.
            "use-forwarded-headers"      = "false"
            "enable-real-ip"             = "false"
            "use-proxy-protocol"         = "false"
            "compute-full-forwarded-for" = "false"
            "disable-ipv6"               = "false"
          }
          # 2026-09-03: a port-22 connection flood pushed nginx to ~5 cores,
          # the controller's healthz could not answer inside the chart's 1 s
          # probe timeout, kubelet killed it, and the 300 s graceful drain
          # closed every public port for four minutes. A spike has to hold
          # for a full minute before that happens now (6 x 10 s, 5 s each),
          # and nginx is capped so it cannot take the whole 8-core node with
          # it. Legit ingress load is ~0.04 cores; the cap only bites in a
          # flood. See CONTEXT.md, incident log.
          livenessProbe = {
            timeoutSeconds   = 5
            failureThreshold = 6
          }
          readinessProbe = {
            timeoutSeconds   = 5
            failureThreshold = 6
          }
          resources = {
            requests = {
              cpu    = "250m"
              memory = "256Mi"
            }
            limits = {
              cpu = "3"
            }
          }
        }
      })
    }
  }

  depends_on = [kubernetes_daemon_set_v1.ssh_edge]
}
