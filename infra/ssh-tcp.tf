# =============================================================================
# SSH TCP Passthrough via NGINX Ingress Controller
# =============================================================================
# Configures the RKE2 built-in NGINX ingress controller to listen on TCP
# entrypoints and forward raw bytes to backend pods, prefixed with PROXY v1
# headers so the backend can see real client IPs.
#
# Two parallel paths during Phase 1–4 of the bastion rollout
# (PERSISTENT-CONNECTION-GATEWAY.md §3 / §10):
#
#   :22    → service-ssh-sv:2222   (legacy in-proc russh)        — production
#   :5222  → service-bastion-sv:5222 (bastion → /tunnel WS)      — dogfood
#
# Phase 5 cutover is a one-line edit: change `:22` to point at
# service-bastion-sv:5222, retire the :5222 entry.
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
        tcp = {
          "22"   = "default/service-ssh-sv:2222::PROXY"
          "5222" = "default/service-bastion-sv:5222::PROXY"
        }
      })
    }
  }
}
