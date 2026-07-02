# =============================================================================
# dopewars door: persistent shared high-score table
# =============================================================================
# dopewars has no mid-game save (upstream single-player has no savegame format),
# so a pod restart ends any in-progress game regardless of where it runs. What
# DOES persist is the high-score table: every session's dopewars child points its
# `-f` at ONE shared file on this PVC, so the leaderboard survives restarts and is
# global across players (the dopewars analog of nethack's shared bones playground).
#
# The dopewars binary creates the .sco file itself on the first score write; we
# only need the mount directory to exist and be writable by the `late` user, which
# the dopewars_score_seed init_container (service-dopewars.tf) ensures.
#
# replicas MUST stay 1: one RWO volume holds the shared score file. This assumes
# the single-node cluster local-path already implies. dopewars locks the score
# file during updates, so concurrent sessions writing it are safe.

locals {
  # DOPEWARS_ENABLED is normalized in defaults.tf; it gates only the CLIENT door
  # (service-ssh's LATE_DOPEWARS_ENABLED). The late-dopewars host pod is always
  # deployed (see service-dopewars.tf).

  # Directory mounted from the PVC; holds the single shared high-score file. MUST
  # match LATE_DOPEWARS_SCORE_FILE's parent baked into the host (see the
  # runtime-dopewars Dockerfile stage and late-dopewars/src/config.rs).
  dopewars_var_path   = "/var/lib/late-dopewars"
  dopewars_score_file = "/var/lib/late-dopewars/dopewars.sco"
  dopewars_pvc_size   = "256Mi"

  # The late-dopewars host pod is reached over the cluster network by service-ssh.
  # Host == the Service name (same namespace, see service-dopewars.tf); port ==
  # the host's SSH listener.
  dopewars_service_host = "late-dopewars-sv"
  dopewars_port         = "2324"
}

# prevent_destroy keeps the leaderboard across redeploys. Mounted by the
# late-dopewars host pod (service-dopewars.tf), which owns the shared score file.
resource "kubernetes_persistent_volume_claim_v1" "dopewars_save" {
  metadata {
    name = "dopewars-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = local.dopewars_pvc_size
      }
    }

    storage_class_name = "local-path"
  }

  wait_until_bound = false

  lifecycle {
    prevent_destroy = true
  }

  depends_on = [
    helm_release.local_path_provisioner
  ]
}
