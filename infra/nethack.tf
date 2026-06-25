# =============================================================================
# NetHack door: persistent writable playground (saves / bones / scores)
# =============================================================================
# The late-ssh image bakes NetHack 5.0.0 with VAR_PLAYGROUND=/var/games/nethack-var
# (see Dockerfile nethack-build stage): read-only data files + the binary stay in
# the image at HACKDIR=/var/games/nethack, while the writable state (per-player
# save/, shared bones, locks, record/logfile) is written under VAR_PLAYGROUND. We
# back only that path with an RWO PVC, so saves and shared bones survive redeploys
# while image rebuilds still ship fresh data files (HACKDIR is never a mount, so
# it is never shadowed).
#
# NetHack never creates its own save/ subdirectory (no mkdir in its source); an
# empty PVC would therefore shadow the image's baked save/ and break saving. The
# nethack_save_seed init_container in service-ssh.tf re-creates save/ and fixes
# ownership on the PVC before late-ssh starts.
#
# replicas MUST stay 1: one RWO volume holds the shared bones + per-player saves.
# This assumes the single-node cluster that local-path (hostPath) already implies
# -- an RWO volume is mountable by every pod on one node, so the existing
# RollingUpdate (max_surge=1) overlap during a deploy is fine, and NetHack's own
# lock files guard the brief two-pod window. Scale-out would need RWX storage or a
# StatefulSet (out of scope).

locals {
  # NETHACK_ENABLED arrives as an empty string from CI when the GitHub variable
  # is unset; default it on. This now gates only the CLIENT door (service-ssh's
  # LATE_NETHACK_ENABLED); the late-nethack host pod is always deployed.
  nethack_enabled = trimspace(var.NETHACK_ENABLED) != "" ? trimspace(var.NETHACK_ENABLED) : "1"

  # MUST equal NETHACK_VAR_PLAYGROUND baked into the binary (Dockerfile).
  nethack_var_path = "/var/games/nethack-var"
  nethack_pvc_size = "2Gi"

  # The late-nethack host pod is reached over the cluster network by service-ssh.
  # Host == the Service name (same namespace, see service-nethack.tf); port == the
  # host's SSH listener.
  nethack_service_host = "late-nethack-sv"
  nethack_port         = "2323"
}

# prevent_destroy is the same belt-and-suspenders the music PVC uses, so the
# saves/bones survive redeploys. Mounted by the late-nethack host pod
# (service-nethack.tf), which owns the writable playground.
resource "kubernetes_persistent_volume_claim_v1" "nethack_save" {
  metadata {
    name = "nethack-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = local.nethack_pvc_size
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
