# =============================================================================
# Usurper door: persistent writable game tree (the one shared world)
# =============================================================================
# The runtime-usurper image bakes USURPER.EXE plus a read-only seed game tree at
# /opt/usurper (see the Dockerfile usurper-build stage). Every child runs with
# its working directory on LATE_USURPER_GAME_DIR: the game resolves everything
# (DATA/ world files, TEXT/ screens, NODE/, per-session DROP/ dropfiles,
# USURPER.CFG) relative to it. We back that directory with an RWO PVC so the
# shared world (players, gangs, king, news) survives redeploys while image
# rebuilds still ship fresh binaries.
#
# The host itself copies missing seed files into the game dir at boot (see
# late-usurper seed.rs); the usurper_save_seed init_container only fixes
# ownership on the PVC before the host starts.
#
# replicas MUST stay 1: one RWO volume holds the single shared world, and the
# game's own file locking assumes one machine. Same single-node local-path
# reasoning as nethack.tf.

locals {
  # USURPER_ENABLED arrives as an empty string from CI when the GitHub variable
  # is unset; default it on. This gates only the CLIENT door (service-ssh's
  # LATE_USURPER_ENABLED); the late-usurper host pod is always deployed.
  usurper_enabled = trimspace(var.USURPER_ENABLED) != "" ? trimspace(var.USURPER_ENABLED) : "1"

  # The writable game tree on the PVC. MUST match LATE_USURPER_GAME_DIR's
  # default baked into the host (see the runtime-usurper Dockerfile stage and
  # late-usurper/src/config.rs).
  usurper_var_path = "/var/lib/late-usurper"
  usurper_pvc_size = "1Gi"

  # The late-usurper host pod is reached over the cluster network by
  # service-ssh. Host == the Service name (same namespace, see
  # service-usurper.tf); port == the host's SSH listener.
  usurper_service_host = "late-usurper-sv"
  usurper_port         = "2326"
}

# prevent_destroy keeps the shared world across redeploys. Mounted by the
# late-usurper host pod (service-usurper.tf), which owns the writable tree.
resource "kubernetes_persistent_volume_claim_v1" "usurper_save" {
  metadata {
    name = "usurper-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = local.usurper_pvc_size
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
