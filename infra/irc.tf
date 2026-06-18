# =============================================================================
# IRC: raw TLS listener for embedded late-ssh ircd
# =============================================================================

resource "kubernetes_manifest" "irc_certificate" {
  count = local.irc_enabled_bool ? 1 : 0

  manifest = {
    apiVersion = "cert-manager.io/v1"
    kind       = "Certificate"
    metadata = {
      name      = "irc-tls"
      namespace = "default"
    }
    spec = {
      secretName = local.irc_tls_secret_name
      dnsNames   = [local.irc_host]
      issuerRef = {
        kind = "ClusterIssuer"
        name = "letsencrypt-prod"
      }
    }
  }

  depends_on = [kubernetes_manifest.cluster_issuer_letsencrypt_prod]
}
