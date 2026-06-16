# =============================================================================
# LORD BBS proof-of-life stack
#
# This is intentionally isolated and disabled by default. It deploys a private
# BBS service for manual V1 testing; late.sh app integration belongs to V2.
# =============================================================================

resource "kubernetes_namespace_v1" "lord_bbs" {
  count = var.LORD_BBS_ENABLED ? 1 : 0

  metadata {
    name = var.LORD_BBS_NAMESPACE
  }
}

resource "kubernetes_persistent_volume_claim_v1" "lord_bbs_data" {
  count = var.LORD_BBS_ENABLED ? 1 : 0

  metadata {
    name      = "lord-bbs-data"
    namespace = kubernetes_namespace_v1.lord_bbs[0].metadata[0].name
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = var.LORD_BBS_STORAGE_SIZE
      }
    }

    storage_class_name = var.LORD_BBS_STORAGE_CLASS
  }

  wait_until_bound = false

  lifecycle {
    prevent_destroy = true
  }

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_deployment_v1" "lord_bbs" {
  count = var.LORD_BBS_ENABLED ? 1 : 0

  metadata {
    name      = "lord-bbs"
    namespace = kubernetes_namespace_v1.lord_bbs[0].metadata[0].name
  }

  spec {
    replicas = 1

    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "lord-bbs"
      }
    }

    template {
      metadata {
        labels = {
          app = "lord-bbs"
        }
      }

      spec {
        termination_grace_period_seconds = 120

        container {
          image = var.LORD_BBS_IMAGE_TAG
          name  = "lord-bbs"

          port {
            container_port = 23
            name           = "telnet"
          }

          port {
            container_port = 22
            name           = "ssh"
          }

          env {
            name  = "SBBS_HOME"
            value = "/bbs/sbbs"
          }

          env {
            name  = "LORD_HOME"
            value = "/bbs/doors/lord"
          }

          resources {
            limits = {
              cpu    = "2000m"
              memory = "2Gi"
            }
            requests = {
              cpu    = "250m"
              memory = "512Mi"
            }
          }

          readiness_probe {
            tcp_socket {
              port = "telnet"
            }
            initial_delay_seconds = 20
            period_seconds        = 10
            failure_threshold     = 12
          }

          liveness_probe {
            tcp_socket {
              port = "telnet"
            }
            initial_delay_seconds = 60
            period_seconds        = 30
            failure_threshold     = 5
          }

          volume_mount {
            name       = "bbs-data"
            mount_path = "/bbs"
          }
        }

        image_pull_secrets {
          name = kubernetes_secret_v1.regcred.metadata[0].name
        }

        volume {
          name = "bbs-data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.lord_bbs_data[0].metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "lord_bbs_sv" {
  count = var.LORD_BBS_ENABLED ? 1 : 0

  metadata {
    name      = "lord-bbs-sv"
    namespace = kubernetes_namespace_v1.lord_bbs[0].metadata[0].name
  }

  spec {
    selector = {
      app = "lord-bbs"
    }

    type = "ClusterIP"

    port {
      name        = "telnet"
      port        = 23
      target_port = "telnet"
    }

    port {
      name        = "ssh"
      port        = 22
      target_port = "ssh"
    }
  }
}
