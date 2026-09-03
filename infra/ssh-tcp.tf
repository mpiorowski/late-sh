# =============================================================================
# SSH and IRC TCP passthrough via NGINX Ingress Controller
# =============================================================================
# Configures the RKE2 built-in NGINX ingress controller to listen on port 22
# and forward raw TCP traffic to the late-ssh pod with PROXY protocol metadata
# so both network frontends can resolve real client IPs.
# This enables: ssh late.sh and irc.late.sh
# =============================================================================

resource "kubernetes_manifest" "nginx_tcp_config" {
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
        tcp = merge(
          {
            "22" = "default/service-ssh-sv:2222::PROXY"
          },
          local.irc_enabled_bool ? {
            tostring(local.irc_port) = "default/service-ssh-sv:${local.irc_port}${local.irc_proxy_emit_bool ? "::PROXY" : ""}"
          } : {}
        )
      })
    }
  }
}
