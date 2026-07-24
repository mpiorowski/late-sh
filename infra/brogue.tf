# =============================================================================
# Brogue door: persistent writable playground (per-player save directories)
# =============================================================================
# The runtime-brogue image bakes the curses-only Brogue CE binary (see the
# Dockerfile brogue-build stage). brogue opens every player file (saves,
# recordings, high scores, run history) relative to its working directory, so
# the late-brogue host runs each child with cwd LATE_BROGUE_DATA_DIR/players/
# <playname>. We back that root with an RWO PVC, so saves survive redeploys
# while image rebuilds still ship a fresh binary.
#
# The host creates each per-player directory on demand; the brogue_save_seed
# init_container only fixes ownership on the PVC before the host starts.
#
# replicas MUST stay 1: one RWO volume holds every player's save. Same
# single-node local-path reasoning as dcss.tf.

locals {
  # BROGUE_ENABLED arrives as an empty string from CI when the GitHub variable
  # is unset; default it on. This gates only the CLIENT door (service-ssh's
  # LATE_BROGUE_ENABLED); the late-brogue host pod is always deployed.
  brogue_enabled = trimspace(var.BROGUE_ENABLED) != "" ? trimspace(var.BROGUE_ENABLED) : "1"

  # The playground root on the PVC; each child runs in players/<playname>
  # under it. MUST match LATE_BROGUE_DATA_DIR's default baked into the host
  # (see the runtime-brogue Dockerfile stage and late-brogue/src/config.rs).
  brogue_var_path = "/var/lib/late-brogue"
  brogue_pvc_size = "2Gi"

  # The late-brogue host pod is reached over the cluster network by
  # service-ssh. Host == the Service name (same namespace, see
  # service-brogue.tf); port == the host's SSH listener.
  brogue_service_host = "late-brogue-sv"
  brogue_port         = "2327"
}

# prevent_destroy keeps the saves across redeploys. Mounted by the late-brogue
# host pod (service-brogue.tf), which owns the writable playground.
resource "kubernetes_persistent_volume_claim_v1" "brogue_save" {
  metadata {
    name = "brogue-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = local.brogue_pvc_size
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
