###############################################################################
# AgentBBS — GCP sysop reporting infrastructure.
#
# Provisions:
#   * Firestore database (native mode) for `agentbbs_events` + `sysop_reports`.
#   * A Pub/Sub topic `agentbbs-events` + a subscription.
#   * A 2nd-gen Cloud Function, triggered by the topic, that folds events into
#     `sysop_reports/latest` (mirrors agentbbs-gcp's ReportAggregator).
#
# Reviewable, not auto-applied. `terraform plan` before any apply.
###############################################################################

terraform {
  required_version = ">= 1.5.0"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 5.0.0"
    }
  }
}

provider "google" {
  project = var.project_id
  region  = var.region
}

# --- Required APIs ----------------------------------------------------------

resource "google_project_service" "services" {
  for_each = toset([
    "firestore.googleapis.com",
    "pubsub.googleapis.com",
    "cloudfunctions.googleapis.com",
    "run.googleapis.com",
    "cloudbuild.googleapis.com",
    "eventarc.googleapis.com",
  ])
  service            = each.value
  disable_on_destroy = false
}

# --- Firestore (native mode) ------------------------------------------------

resource "google_firestore_database" "default" {
  project     = var.project_id
  name        = "(default)"
  location_id = var.firestore_location
  type        = "FIRESTORE_NATIVE"

  depends_on = [google_project_service.services]
}

# --- Pub/Sub topic + subscription ------------------------------------------

resource "google_pubsub_topic" "events" {
  name       = var.topic_name
  depends_on = [google_project_service.services]
}

resource "google_pubsub_subscription" "events" {
  name  = var.subscription_name
  topic = google_pubsub_topic.events.id

  ack_deadline_seconds       = 20
  message_retention_duration = "86400s"
  retain_acked_messages      = false

  expiration_policy {
    ttl = "" # never expire
  }
}

# --- Cloud Function (2nd gen), Pub/Sub triggered ----------------------------

resource "google_cloudfunctions2_function" "sysop_report" {
  name        = var.function_name
  location    = var.region
  description = "Folds AgentBBS Pub/Sub events into sysop_reports/latest in Firestore."

  build_config {
    runtime     = var.function_runtime
    entry_point = "aggregateSysopReport"
    source {
      storage_source {
        bucket = var.function_source_bucket
        object = var.function_source_object
      }
    }
  }

  service_config {
    max_instance_count    = 3
    min_instance_count    = 0
    available_memory      = "256M"
    timeout_seconds       = 60
    ingress_settings      = "ALLOW_INTERNAL_ONLY"
    environment_variables = {
      GOOGLE_CLOUD_PROJECT = var.project_id
    }
  }

  event_trigger {
    trigger_region = var.region
    event_type     = "google.cloud.pubsub.topic.v1.messagePublished"
    pubsub_topic   = google_pubsub_topic.events.id
    retry_policy   = "RETRY_POLICY_RETRY"
  }

  depends_on = [google_project_service.services]
}
