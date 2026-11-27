# cert-manager (required by barman-cloud plugin for TLS)
# https://cert-manager.io/docs/installation/helm/
resource "helm_release" "cert_manager" {
  name             = "cert-manager"
  repository       = "https://charts.jetstack.io"
  chart            = "cert-manager"
  version          = "1.19.2"
  namespace        = "cert-manager"
  create_namespace = true

  values = [
    yamlencode({
      crds = {
        enabled = true
      }
    })
  ]
}

# CloudNativePG Operator
# https://github.com/cloudnative-pg/charts
resource "helm_release" "cloudnativepg" {
  name             = "cnpg"
  repository       = "https://cloudnative-pg.github.io/charts"
  chart            = "cloudnative-pg"
  version          = "0.27.0"
  namespace        = "cnpg-system"
  create_namespace = true
}

# Barman Cloud Plugin for backups
# https://github.com/cloudnative-pg/charts/releases
resource "helm_release" "barman_cloud_plugin" {
  name       = "barman-cloud"
  repository = "https://cloudnative-pg.github.io/charts"
  chart      = "plugin-barman-cloud"
  version    = "0.4.0"
  namespace  = "cnpg-system"

  depends_on = [helm_release.cloudnativepg, helm_release.cert_manager]
}

# ObjectStore for R2 backup configuration
resource "kubernetes_manifest" "postgres_object_store" {
  manifest = {
    apiVersion = "barmancloud.cnpg.io/v1"
    kind       = "ObjectStore"
    metadata = {
      name      = "s3-backup-store"
      namespace = "default"
    }
    spec = {
      configuration = {
        destinationPath = "s3://${var.DB_BACKUPS_BUCKET}/"
        endpointURL     = var.S3_ENDPOINT
        s3Credentials = {
          accessKeyId = {
            name = kubernetes_secret_v1.s3_credentials.metadata[0].name
            key  = "ACCESS_KEY_ID"
          }
          secretAccessKey = {
            name = kubernetes_secret_v1.s3_credentials.metadata[0].name
            key  = "SECRET_ACCESS_KEY"
          }
        }
        wal = {
          compression = "gzip"
        }
        data = {
          compression = "gzip"
          jobs        = 2
        }
      }
      retentionPolicy = "7d"
      instanceSidecarConfiguration = {
        retentionPolicyIntervalSeconds = 1800
        resources = {
          requests = {
            cpu    = "50m"
            memory = "128Mi"
          }
          limits = {
            cpu    = "500m"
            memory = "512Mi"
          }
        }
      }
    }
  }

  depends_on = [helm_release.barman_cloud_plugin]
}

# PostgreSQL Cluster
resource "kubernetes_manifest" "postgres_cluster" {
  computed_fields = ["spec.postgresql.parameters"]
  manifest = {
    apiVersion = "postgresql.cnpg.io/v1"
    kind       = "Cluster"
    metadata = {
      name      = "postgres"
      namespace = "default"
    }
    spec = {
      instances = 2

      # glibc starts by giving every allocation of 128 kB or more its own
      # mapping, returned to the kernel when freed, but raises that line
      # to the size of the first such block a process frees (up to 32 MB).
      # From then on a backend's multi-megabyte buffers (an art board
      # snapshot is 7 MB on the wire) come out of its heap, and one cache
      # entry allocated above a buffer keeps the hole for the life of the
      # connection. Setting the threshold pins it: large buffers always go
      # back (infra/RUNBOOK.md, 2026-11-14). Changing this restarts
      # Postgres.
      env = [
        {
          name  = "MALLOC_MMAP_THRESHOLD_"
          value = "131072"
        }
      ]

      storage = {
        size         = "10Gi"
        storageClass = "local-path"
      }

      resources = {
        requests = {
          memory = "768Mi"
          cpu    = "200m"
        }
        limits = {
          memory = "4Gi"
          cpu    = "1"
        }
      }

      postgresql = {
        parameters = {
          shared_buffers             = "256MB"
          max_connections            = "100"
          "pg_stat_statements.max"   = "10000"
          "pg_stat_statements.track" = "all"
          track_io_timing            = "on"
          # Short OLTP queries only, so JIT costs more than it saves: 5% of DB
          # time, 11% of the top query (SCALE.md, DB Cost Ranking).
          jit = "off"
        }
      }

      bootstrap = {
        initdb = {
          database = "app"
          owner    = "app"
        }
      }

      plugins = [
        {
          name          = "barman-cloud.cloudnative-pg.io"
          isWALArchiver = true
          parameters = {
            barmanObjectName = "s3-backup-store"
          }
        }
      ]
    }
  }

  depends_on = [
    helm_release.cloudnativepg,
    kubernetes_manifest.postgres_object_store
  ]
}

# Scheduled daily backup
resource "kubernetes_manifest" "postgres_scheduled_backup" {
  manifest = {
    apiVersion = "postgresql.cnpg.io/v1"
    kind       = "ScheduledBackup"
    metadata = {
      name      = "postgres-daily-backup"
      namespace = "default"
    }
    spec = {
      schedule             = "0 0 2 * * *" # Daily at 2:00 AM (sec min hour dom mon dow)
      backupOwnerReference = "self"
      cluster = {
        name = "postgres"
      }
      method = "plugin"
      pluginConfiguration = {
        name = "barman-cloud.cloudnative-pg.io"
      }
    }
  }

  depends_on = [kubernetes_manifest.postgres_cluster]
}
