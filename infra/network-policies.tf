# =============================================================================
# NetworkPolicies — defense-in-depth around the bastion ⇄ late-ssh trust seam.
#
# Layer 1 in the trust model from devdocs/LATE-CONNECTION-BASTION.md §7.
# Combined with the in-app IP allowlist (Layer 2) and pre-shared secret
# (Layer 3), this means a future Service/VPC typo cannot silently expose
# late-ssh's /tunnel to anything other than the bastion.
#
# Footgun reminder: as soon as ANY policy selects a pod for "Ingress",
# the pod flips from default-allow to default-deny on ingress; only what
# the policies explicitly allow gets through. This policy therefore must
# also re-allow legitimate traffic to :2222 and :4000.
#
# Policy semantics used below:
#   - ingress block with `ports` and no `from` → allow from any source on
#     those ports.
#   - ingress block with `from` (a pod_selector) and `ports` → allow only
#     from matching pods on those ports.
# =============================================================================

resource "kubernetes_network_policy_v1" "service_ssh_ingress" {
  metadata {
    name = "service-ssh-ingress"
  }

  spec {
    pod_selector {
      match_labels = {
        app = "service-ssh"
      }
    }

    policy_types = ["Ingress"]

    # :2222 (SSH legacy path) and :4000 (HTTP API) — open to any source,
    # matching the prior default-allow behavior.
    ingress {
      ports {
        port     = 2222
        protocol = "TCP"
      }

      ports {
        port     = 4000
        protocol = "TCP"
      }
    }

    # :4001 (/tunnel) — only the bastion may reach it. Layer 1 of the
    # tunnel trust model.
    ingress {
      from {
        pod_selector {
          match_labels = {
            app = "service-bastion"
          }
        }
      }

      ports {
        port     = 4001
        protocol = "TCP"
      }
    }
  }
}
