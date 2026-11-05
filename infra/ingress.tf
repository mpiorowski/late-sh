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

# audio.late.sh/admin → 403 for everyone. Icecast's admin pages take the admin
# password over the public internet, and nothing outside the cluster needs them:
# Liquidsoap and late-ssh reach Icecast through icecast-sv, which skips nginx.
# A second Ingress because an allowlist annotation covers its whole Ingress, and
# the stream itself on the one above stays public. The range is TEST-NET-1
# (RFC 5737), so no client can ever match it.
resource "kubernetes_ingress_v1" "icecast_admin_blocked" {
  metadata {
    name = "icecast-admin-blocked"
    annotations = {
      "kubernetes.io/ingress.class"                        = "nginx"
      "nginx.ingress.kubernetes.io/whitelist-source-range" = "192.0.2.1/32"
    }
  }

  spec {
    rule {
      host = "audio.${local.domain}"
      http {
        path {
          path      = "/admin"
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
# nginx reload never touches them. depends_on hands port 22 over in order: the
# chart drops nginx's hostPort DNAT only once ssh-edge is up to take it.

locals {
  # https://www.cloudflare.com/ips/. A range missing here only means that edge
  # is taken for the client, which is safe and what every request did before.
  cloudflare_cidrs = [
    "173.245.48.0/20",
    "103.21.244.0/22",
    "103.22.200.0/22",
    "103.31.4.0/22",
    "141.101.64.0/18",
    "108.162.192.0/18",
    "190.93.240.0/20",
    "188.114.96.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
    "162.158.0.0/15",
    "104.16.0.0/13",
    "104.24.0.0/14",
    "172.64.0.0/13",
    "131.0.72.0/22",
    "2400:cb00::/32",
    "2606:4700::/32",
    "2803:f800::/32",
    "2405:b500::/32",
    "2405:8100::/32",
    "2a06:98c0::/29",
    "2c0f:f248::/32",
  ]
}

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
          # api and audio sit behind Cloudflare, so nginx's peer is an edge and
          # late-ssh's per-IP pair limits would count edges, not people. Take
          # the client from CF-Connecting-IP, only when an edge sent it, so a
          # direct request cannot forge it. Deliberately not
          # use-forwarded-headers: that also trusts X-Forwarded-Host and Proto
          # from anyone. X-Forwarded-For to the backends becomes the client.
          config = {
            "enable-real-ip"       = "true"
            "forwarded-for-header" = "CF-Connecting-IP"
            "proxy-real-ip-cidr"   = join(",", local.cloudflare_cidrs)
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
