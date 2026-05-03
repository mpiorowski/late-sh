# =============================================================================
# IPv6 edge proxy
# =============================================================================
# The RKE2/Canal hostPort path creates IPv4 DNAT rules only because the cluster
# pod and service CIDRs are IPv4-only. This host-network HAProxy binds only the
# node's public IPv6 address and forwards traffic into the existing IPv4 path.
# It does not bind IPv4, so the current ingress-nginx hostPort setup remains
# responsible for 46.62.210.86:80/443/22.
# =============================================================================

resource "kubernetes_config_map_v1" "ipv6_proxy" {
  count = var.IPV6_PROXY_ENABLED ? 1 : 0

  metadata {
    name      = "ipv6-proxy"
    namespace = "kube-system"
  }

  data = {
    "haproxy.cfg" = <<-EOT
      global
        log stdout format raw local0
        maxconn 20000

      defaults
        mode tcp
        log global
        option tcplog
        timeout connect 5s
        timeout client 12h
        timeout server 12h

      frontend http_ipv6
        bind [${var.IPV6_PROXY_ADDRESS}]:80 v6only
        default_backend ingress_http_ipv4

      backend ingress_http_ipv4
        server ingress_http 127.0.0.1:80 check inter 5s fall 2 rise 2

      frontend https_ipv6
        bind [${var.IPV6_PROXY_ADDRESS}]:443 v6only
        default_backend ingress_https_ipv4

      backend ingress_https_ipv4
        server ingress_https 127.0.0.1:443 check inter 5s fall 2 rise 2

      frontend ssh_ipv6
        bind [${var.IPV6_PROXY_ADDRESS}]:22 v6only
        default_backend service_ssh_ipv4

      backend service_ssh_ipv4
        server service_ssh service-ssh-sv.default.svc.cluster.local:2222 send-proxy
    EOT
  }
}

resource "kubernetes_daemon_set_v1" "ipv6_proxy" {
  count = var.IPV6_PROXY_ENABLED ? 1 : 0

  metadata {
    name      = "ipv6-proxy"
    namespace = "kube-system"

    labels = {
      app = "ipv6-proxy"
    }
  }

  spec {
    selector {
      match_labels = {
        app = "ipv6-proxy"
      }
    }

    template {
      metadata {
        labels = {
          app = "ipv6-proxy"
        }
      }

      spec {
        host_network                     = true
        dns_policy                       = "ClusterFirstWithHostNet"
        termination_grace_period_seconds = 10

        node_selector = {
          "kubernetes.io/os" = "linux"
        }

        container {
          name    = "haproxy"
          image   = var.IPV6_PROXY_IMAGE
          command = ["haproxy"]
          args    = ["-f", "/usr/local/etc/haproxy/haproxy.cfg", "-db"]

          resources {
            requests = {
              cpu    = "25m"
              memory = "32Mi"
            }
            limits = {
              cpu    = "250m"
              memory = "256Mi"
            }
          }

          security_context {
            allow_privilege_escalation = false
            run_as_non_root            = false
            run_as_user                = 0
            run_as_group               = 0

            capabilities {
              add  = ["NET_BIND_SERVICE"]
              drop = ["ALL"]
            }
          }

          volume_mount {
            name       = "config"
            mount_path = "/usr/local/etc/haproxy"
            read_only  = true
          }
        }

        volume {
          name = "config"

          config_map {
            name = kubernetes_config_map_v1.ipv6_proxy[0].metadata[0].name
          }
        }
      }
    }

    strategy {
      type = "RollingUpdate"

      rolling_update {
        max_unavailable = 1
      }
    }
  }
}
