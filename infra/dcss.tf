# =============================================================================
# DCSS door: persistent writable playground (per-player saves / morgue / scores)
# =============================================================================
# The runtime-dcss image bakes crawl 0.34 with the default SAVEDIR=~/.crawl (see
# Dockerfile dcss-build stage): the read-only data tree stays in the image at
# /opt/dcss, while every child runs with HOME=LATE_DCSS_DATA_DIR, so all writable
# state (per-player saves keyed by the `-name` playname, shared scores/logfile/
# milestones, morgue dumps) lands under $HOME/.crawl. We back that HOME with an
# RWO PVC, so saves survive redeploys while image rebuilds still ship fresh data
# files.
#
# crawl creates its own ~/.crawl tree on first run; the dcss_save_seed
# init_container only fixes ownership on the PVC before the host starts.
#
# replicas MUST stay 1: one RWO volume holds every player's save. Same
# single-node local-path reasoning as nethack.tf; crawl's own per-player save
# locking guards a same-account double launch.

locals {
  # DCSS_ENABLED arrives as an empty string from CI when the GitHub variable is
  # unset; default it on. This gates only the CLIENT door (service-ssh's
  # LATE_DCSS_ENABLED); the late-dcss host pod is always deployed.
  dcss_enabled = trimspace(var.DCSS_ENABLED) != "" ? trimspace(var.DCSS_ENABLED) : "1"

  # The child HOME on the PVC; crawl writes everything under $HOME/.crawl. MUST
  # match LATE_DCSS_DATA_DIR's default baked into the host (see the runtime-dcss
  # Dockerfile stage and late-dcss/src/config.rs).
  dcss_var_path = "/var/lib/late-dcss"
  dcss_pvc_size = "2Gi"

  # The late-dcss host pod is reached over the cluster network by service-ssh.
  # Host == the Service name (same namespace, see service-dcss.tf); port == the
  # host's SSH listener.
  dcss_service_host = "late-dcss-sv"
  dcss_port         = "2325"
}

# prevent_destroy keeps the saves across redeploys. Mounted by the late-dcss
# host pod (service-dcss.tf), which owns the writable playground.
resource "kubernetes_persistent_volume_claim_v1" "dcss_save" {
  metadata {
    name = "dcss-save"
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = local.dcss_pvc_size
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
