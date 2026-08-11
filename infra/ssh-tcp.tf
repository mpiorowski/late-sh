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

  lifecycle {
    precondition {
      condition     = !local.irc_enabled_bool || !local.irc_proxy_emit_bool || local.irc_proxy_accept_bool
      error_message = "IRC_PROXY_EMIT requires IRC_PROXY_ACCEPT while IRC is enabled. Deploy parser acceptance before enabling proxy emission."
    }
  }
}
