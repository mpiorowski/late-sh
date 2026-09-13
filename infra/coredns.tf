# =============================================================================
# CoreDNS: values override for RKE2's bundled rke2-coredns chart
# =============================================================================
# The chart's autoscaler runs one CoreDNS replica per node, at least two once
# a second node exists (preventSinglePointFailure), and a required pod
# anti-affinity puts each replica on a different node. agent-1 carries the
# support NoSchedule taint (defaults.tf, node placement), so without this
# toleration the second replica sits Pending forever and every DNS lookup in
# the cluster depends on the single replica on server-1.
#
# `tolerations` replaces the chart's list rather than appending, so the two
# chart defaults are restated here (rke2-coredns 1.45.201 values). Re-check
# them after an RKE2 upgrade. CriticalAddonsOnly comes from the chart template
# itself (isClusterService), not from this list.
# =============================================================================

resource "kubernetes_manifest" "coredns_config" {
  manifest = {
    apiVersion = "helm.cattle.io/v1"
    kind       = "HelmChartConfig"
    metadata = {
      name      = "rke2-coredns"
      namespace = "kube-system"
    }
    spec = {
      valuesContent = yamlencode({
        tolerations = [
          {
            key      = "node-role.kubernetes.io/control-plane"
            operator = "Exists"
            effect   = "NoSchedule"
          },
          {
            key      = "node-role.kubernetes.io/etcd"
            operator = "Exists"
            effect   = "NoExecute"
          },
          {
            key      = local.support_node_label_key
            operator = "Equal"
            value    = local.support_node_label_value
            effect   = "NoSchedule"
          },
        ]
      })
    }
  }
}
