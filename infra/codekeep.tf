# CodeKeep stores one autosaved campaign beneath each account-specific HOME.
# One RWO PVC is owned by the single late-codekeep host replica.
locals {
  codekeep_var_path     = "/var/lib/late-codekeep"
  codekeep_pvc_size     = "1Gi"
  codekeep_service_host = "late-codekeep-sv"
  codekeep_port         = "2328"
}

resource "kubernetes_persistent_volume_claim_v1" "codekeep_save" {
  metadata {
    name = "codekeep-save"
  }

  spec {
    access_modes       = ["ReadWriteOnce"]
    storage_class_name = "local-path"
    resources {
      requests = {
        storage = local.codekeep_pvc_size
      }
    }
  }

  lifecycle {
    prevent_destroy = true
  }
}
