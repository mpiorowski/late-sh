# =============================================================================
# BashQuest door: persistent shared writable playground (users.db + saves)
# =============================================================================
# bashquest.sh keeps everything under $HOME/.bashquest: the shared users.db
# and every player's <name>.save, plus each graduate's certificate file. Every
# child runs with HOME=LATE_BASHQUEST_DATA_DIR, a single shared directory
# (not per-player, unlike nethack/dcss), because bashquest.sh's own in-game
# leaderboard only means something if every player's save lands in the same
# place. bashquest.sh creates users.db and every save file itself on first
# write; the bashquest_save_seed init_container only fixes ownership on the
# PVC before the host starts.
#
# replicas MUST stay 1: one RWO volume holds every player's save. Same
# single-node local-path reasoning as dcss.tf; bashquest.sh saves continuously
# rather than only on exit (see save_progress() call sites), so there is no
# hangup-save/lock concern the way a real multi-writer game would have.

locals {
  # BASHQUEST_ENABLED arrives as an empty string from CI when the GitHub
  # variable is unset; default it on. This gates only the CLIENT door
  # (service-ssh's LATE_BASHQUEST_ENABLED); the late-bashquest host pod is
  # always deployed.
  bashquest_enabled = trimspace(var.BASHQUEST_ENABLED) != "" ? trimspace(var.BASHQUEST_ENABLED) : "1"

  # The child HOME on the PVC; bashquest.sh writes everything under
  # $HOME/.bashquest. MUST match LATE_BASHQUEST_DATA_DIR's default baked into
  # the host (see the runtime-bashquest Dockerfile stage and
  # late-bashquest/src/config.rs).
  bashquest_var_path = "/var/lib/late-bashquest"
  bashquest_pvc_size = "256Mi"

  # The late-bashquest host pod is reached over the cluster network by
  # service-ssh. Host == the Service name (same namespace, see
  # service-bashquest.tf); port == the host's SSH listener.
  bashquest_service_host = "late-bashquest-sv"
  bashquest_port         = "2329"
}

# prevent_destroy keeps every player's progress across redeploys. Mounted by
# the late-bashquest host pod (service-bashquest.tf), which owns the shared
# writable playground.
resource "kubernetes_persistent_volume_claim_v1" "bashquest_save" {
  metadata {
    name = "bashquest-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = local.bashquest_pvc_size
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
