resource "helm_release" "local_path_provisioner" {
  name             = "local-path-provisioner"
  repository       = "https://containeroo.github.io/helm-charts/"
  chart            = "local-path-provisioner"
  version          = "0.0.33"
  namespace        = "local-path-storage"
  create_namespace = true

  values = [
    <<-EOT
storageClass:
  defaultClass: true
EOT
  ]
}

resource "kubernetes_namespace_v1" "monitoring" {
  metadata {
    name = "monitoring"
  }
}

data "http" "grafana_dashboard_kubernetes_cluster" {
  url = "https://grafana.com/api/dashboards/14205/revisions/1/download"

  request_headers = {
    Accept = "application/json"
  }
}

locals {
  grafana_kubernetes_cluster_dashboard = replace(
    data.http.grafana_dashboard_kubernetes_cluster.response_body,
    "$${DS_PROMETHEUS}",
    "VictoriaMetrics"
  )
}

resource "kubernetes_config_map_v1" "otel_collector_config" {
  metadata {
    name      = "otel-collector-config"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  data = {
    "config.yaml" = file("${path.module}/../monitoring/otel-collector-config.yaml")
  }
}

resource "kubernetes_config_map_v1" "grafana_datasources" {
  metadata {
    name      = "grafana-datasources"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  data = {
    "datasources.yaml" = file("${path.module}/../monitoring/grafana-datasources.yaml")
  }
}

resource "kubernetes_config_map_v1" "grafana_dashboards_config" {
  metadata {
    name      = "grafana-dashboards-config"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  data = {
    "dashboards.yaml" = file("${path.module}/../monitoring/grafana-dashboards.yaml")
  }
}

resource "kubernetes_config_map_v1" "grafana_dashboards" {
  metadata {
    name      = "grafana-dashboards"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  data = {
    "observability.json"      = file("${path.module}/../monitoring/dashboards/observability.json")
    "kubernetes-cluster.json" = local.grafana_kubernetes_cluster_dashboard
  }
}

resource "helm_release" "vmagent" {
  name       = "vmagent"
  repository = "https://victoriametrics.github.io/helm-charts/"
  chart      = "victoria-metrics-agent"
  version    = "0.36.0"
  namespace  = kubernetes_namespace_v1.monitoring.metadata[0].name

  values = [
    yamlencode({
      fullnameOverride = "vmagent"

      remoteWrite = [
        {
          url = "http://victoriametrics.monitoring.svc.cluster.local:8428/api/v1/write"
        }
      ]

      resources = {
        limits = {
          cpu    = "250m"
          memory = "256Mi"
        }
        requests = {
          cpu    = "50m"
          memory = "128Mi"
        }
      }
    })
  ]

  depends_on = [
    kubernetes_deployment_v1.victoriametrics
  ]
}

resource "kubernetes_persistent_volume_claim_v1" "victoriametrics_data" {
  metadata {
    name      = "victoriametrics-data"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = "10Gi"
      }
    }

    storage_class_name = "local-path"
  }

  wait_until_bound = false

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_persistent_volume_claim_v1" "victorialogs_data" {
  metadata {
    name      = "victorialogs-data"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = "8Gi"
      }
    }

    storage_class_name = "local-path"
  }

  wait_until_bound = false

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_persistent_volume_claim_v1" "victoriatraces_data" {
  metadata {
    name      = "victoriatraces-data"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = "8Gi"
      }
    }

    storage_class_name = "local-path"
  }

  wait_until_bound = false

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_persistent_volume_claim_v1" "grafana_data" {
  metadata {
    name      = "grafana-data"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
  }

  spec {
    access_modes = ["ReadWriteOnce"]

    resources {
      requests = {
        storage = "2Gi"
      }
    }

    storage_class_name = "local-path"
  }

  wait_until_bound = false

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

resource "kubernetes_deployment_v1" "otel_collector" {
  metadata {
    name      = "otel-collector"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "otel-collector"
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = "otel-collector"
      }
    }

    template {
      metadata {
        labels = {
          app = "otel-collector"
        }
        annotations = {
          config_hash = sha256(join("", values(kubernetes_config_map_v1.otel_collector_config.data)))
        }
      }

      spec {
        container {
          name  = "otel-collector"
          image = "otel/opentelemetry-collector-contrib:0.147.0"
          args  = ["--config=/etc/otelcol-contrib/config.yaml"]

          env {
            name  = "TRACES_ENDPOINT"
            value = "victoriatraces.monitoring.svc.cluster.local:4317"
          }

          env {
            name  = "LOGS_ENDPOINT"
            value = "http://victorialogs.monitoring.svc.cluster.local:9428/insert/opentelemetry/v1/logs"
          }

          env {
            name  = "METRICS_ENDPOINT"
            value = "http://victoriametrics.monitoring.svc.cluster.local:8428/api/v1/write"
          }

          port {
            name           = "otlp-grpc"
            container_port = 4317
          }

          port {
            name           = "otlp-http"
            container_port = 4318
          }

          port {
            name           = "health"
            container_port = 13133
          }

          resources {
            limits = {
              cpu    = "200m"
              memory = "256Mi"
            }
            requests = {
              cpu    = "25m"
              memory = "64Mi"
            }
          }

          liveness_probe {
            http_get {
              path = "/"
              port = "health"
            }
            initial_delay_seconds = 30
            period_seconds        = 20
          }

          readiness_probe {
            http_get {
              path = "/"
              port = "health"
            }
            initial_delay_seconds = 20
            period_seconds        = 10
          }

          volume_mount {
            name       = "config"
            mount_path = "/etc/otelcol-contrib"
            read_only  = true
          }
        }

        volume {
          name = "config"

          config_map {
            name = kubernetes_config_map_v1.otel_collector_config.metadata[0].name

            items {
              key  = "config.yaml"
              path = "config.yaml"
            }
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "otel_collector" {
  metadata {
    name      = "otel-collector"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "otel-collector"
    }
  }

  spec {
    selector = {
      app = "otel-collector"
    }

    port {
      name        = "otlp-grpc"
      port        = 4317
      target_port = "otlp-grpc"
    }

    port {
      name        = "otlp-http"
      port        = 4318
      target_port = "otlp-http"
    }

  }
}

resource "kubernetes_deployment_v1" "victorialogs" {
  metadata {
    name      = "victorialogs"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "victorialogs"
    }
  }

  spec {
    replicas = 1

    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "victorialogs"
      }
    }

    template {
      metadata {
        labels = {
          app = "victorialogs"
        }
      }

      spec {
        container {
          name  = "victorialogs"
          image = "victoriametrics/victoria-logs:v1.47.0"
          args = [
            "--storageDataPath=/victoria-logs-data",
            "--retentionPeriod=7d"
          ]

          port {
            name           = "http"
            container_port = 9428
          }

          resources {
            limits = {
              cpu    = "250m"
              memory = "384Mi"
            }
            requests = {
              cpu    = "25m"
              memory = "128Mi"
            }
          }

          liveness_probe {
            http_get {
              path = "/health"
              port = "http"
            }
            initial_delay_seconds = 45
            period_seconds        = 20
          }

          readiness_probe {
            http_get {
              path = "/health"
              port = "http"
            }
            initial_delay_seconds = 30
            period_seconds        = 10
          }

          volume_mount {
            name       = "data"
            mount_path = "/victoria-logs-data"
          }
        }

        volume {
          name = "data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.victorialogs_data.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "victorialogs" {
  metadata {
    name      = "victorialogs"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "victorialogs"
    }
  }

  spec {
    selector = {
      app = "victorialogs"
    }

    port {
      name        = "http"
      port        = 9428
      target_port = "http"
    }
  }
}

resource "kubernetes_deployment_v1" "victoriatraces" {
  metadata {
    name      = "victoriatraces"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "victoriatraces"
    }
  }

  spec {
    replicas = 1

    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "victoriatraces"
      }
    }

    template {
      metadata {
        labels = {
          app = "victoriatraces"
        }
      }

      spec {
        container {
          name  = "victoriatraces"
          image = "victoriametrics/victoria-traces:v0.7.1"
          args = [
            "--storageDataPath=/victoria-traces-data",
            "--retentionPeriod=3d",
            "--otlpGRPCListenAddr=:4317",
            "--otlpGRPC.tls=false",
            "--httpListenAddr=:10428",
            "--servicegraph.enableTask=true"
          ]

          port {
            name           = "http"
            container_port = 10428
          }

          port {
            name           = "otlp-grpc"
            container_port = 4317
          }

          resources {
            limits = {
              cpu    = "250m"
              memory = "512Mi"
            }
            requests = {
              cpu    = "50m"
              memory = "128Mi"
            }
          }

          liveness_probe {
            http_get {
              path = "/health"
              port = "http"
            }
            initial_delay_seconds = 45
            period_seconds        = 20
          }

          readiness_probe {
            http_get {
              path = "/health"
              port = "http"
            }
            initial_delay_seconds = 30
            period_seconds        = 10
          }

          volume_mount {
            name       = "data"
            mount_path = "/victoria-traces-data"
          }
        }

        volume {
          name = "data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.victoriatraces_data.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "victoriatraces" {
  metadata {
    name      = "victoriatraces"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "victoriatraces"
    }
  }

  spec {
    selector = {
      app = "victoriatraces"
    }

    port {
      name        = "http"
      port        = 10428
      target_port = "http"
    }

    port {
      name        = "otlp-grpc"
      port        = 4317
      target_port = "otlp-grpc"
    }

  }
}

resource "kubernetes_deployment_v1" "victoriametrics" {
  metadata {
    name      = "victoriametrics"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "victoriametrics"
    }
  }

  spec {
    replicas = 1

    strategy {
      type = "Recreate"
    }

    selector {
      match_labels = {
        app = "victoriametrics"
      }
    }

    template {
      metadata {
        labels = {
          app = "victoriametrics"
        }
      }

      spec {
        container {
          name  = "victoriametrics"
          image = "victoriametrics/victoria-metrics:v1.137.0"
          args = [
            "--storageDataPath=/storage",
            "--retentionPeriod=1"
          ]

          port {
            name           = "http"
            container_port = 8428
          }

          resources {
            limits = {
              cpu    = "500m"
              memory = "512Mi"
            }
            requests = {
              cpu    = "50m"
              memory = "128Mi"
            }
          }

          liveness_probe {
            http_get {
              path = "/health"
              port = "http"
            }
            initial_delay_seconds = 45
            period_seconds        = 20
          }

          readiness_probe {
            http_get {
              path = "/health"
              port = "http"
            }
            initial_delay_seconds = 30
            period_seconds        = 10
          }

          volume_mount {
            name       = "data"
            mount_path = "/storage"
          }
        }

        volume {
          name = "data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.victoriametrics_data.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "victoriametrics" {
  metadata {
    name      = "victoriametrics"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "victoriametrics"
    }
  }

  spec {
    selector = {
      app = "victoriametrics"
    }

    port {
      name        = "http"
      port        = 8428
      target_port = "http"
    }
  }
}

resource "kubernetes_deployment_v1" "grafana" {
  metadata {
    name      = "grafana"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "grafana"
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = "grafana"
      }
    }

    template {
      metadata {
        labels = {
          app = "grafana"
        }
        annotations = {
          config_hash = sha256(join("", [
            join("", values(kubernetes_config_map_v1.grafana_datasources.data)),
            join("", values(kubernetes_config_map_v1.grafana_dashboards_config.data)),
            join("", values(kubernetes_config_map_v1.grafana_dashboards.data)),
          ]))
        }
      }

      spec {
        container {
          name  = "grafana"
          image = "grafana/grafana-enterprise:12.4.0"

          env {
            name = "GF_SECURITY_ADMIN_USER"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.grafana_admin.metadata[0].name
                key  = "username"
              }
            }
          }

          env {
            name = "GF_SECURITY_ADMIN_PASSWORD"
            value_from {
              secret_key_ref {
                name = kubernetes_secret_v1.grafana_admin.metadata[0].name
                key  = "password"
              }
            }
          }

          env {
            name  = "GF_AUTH_ANONYMOUS_ENABLED"
            value = "false"
          }

          env {
            name  = "GF_AUTH_DISABLE_LOGIN_FORM"
            value = "false"
          }

          env {
            name  = "GF_INSTALL_PLUGINS"
            value = "victoriametrics-metrics-datasource,victoriametrics-logs-datasource"
          }

          port {
            name           = "http"
            container_port = 3000
          }

          resources {
            limits = {
              cpu    = "200m"
              memory = "256Mi"
            }
            requests = {
              cpu    = "25m"
              memory = "128Mi"
            }
          }

          liveness_probe {
            http_get {
              path = "/api/health"
              port = "http"
            }
            initial_delay_seconds = 45
            period_seconds        = 20
          }

          readiness_probe {
            http_get {
              path = "/api/health"
              port = "http"
            }
            initial_delay_seconds = 30
            period_seconds        = 10
          }

          volume_mount {
            name       = "grafana-datasources"
            mount_path = "/etc/grafana/provisioning/datasources/datasources.yaml"
            sub_path   = "datasources.yaml"
            read_only  = true
          }

          volume_mount {
            name       = "grafana-dashboards-config"
            mount_path = "/etc/grafana/provisioning/dashboards/dashboards.yaml"
            sub_path   = "dashboards.yaml"
            read_only  = true
          }

          volume_mount {
            name       = "grafana-dashboards"
            mount_path = "/etc/grafana/provisioning/dashboards/starter"
            read_only  = true
          }

          volume_mount {
            name       = "data"
            mount_path = "/var/lib/grafana"
          }
        }

        volume {
          name = "grafana-datasources"

          config_map {
            name = kubernetes_config_map_v1.grafana_datasources.metadata[0].name
          }
        }

        volume {
          name = "grafana-dashboards-config"

          config_map {
            name = kubernetes_config_map_v1.grafana_dashboards_config.metadata[0].name
          }
        }

        volume {
          name = "grafana-dashboards"

          config_map {
            name = kubernetes_config_map_v1.grafana_dashboards.metadata[0].name
          }
        }

        volume {
          name = "data"

          persistent_volume_claim {
            claim_name = kubernetes_persistent_volume_claim_v1.grafana_data.metadata[0].name
          }
        }
      }
    }
  }
}

resource "kubernetes_service_v1" "grafana" {
  metadata {
    name      = "grafana"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    labels = {
      app = "grafana"
    }
  }

  spec {
    selector = {
      app = "grafana"
    }

    port {
      name        = "http"
      port        = 3000
      target_port = "http"
    }
  }
}

resource "kubernetes_ingress_v1" "grafana" {
  metadata {
    name      = "grafana"
    namespace = kubernetes_namespace_v1.monitoring.metadata[0].name
    annotations = {
      "kubernetes.io/ingress.class" = "nginx"
    }
  }

  spec {
    rule {
      host = var.GRAFANA_URL

      http {
        path {
          path      = "/"
          path_type = "Prefix"

          backend {
            service {
              name = kubernetes_service_v1.grafana.metadata[0].name

              port {
                number = 3000
              }
            }
          }
        }
      }
    }
  }
}
