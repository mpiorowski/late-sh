output "firestore_database" {
  description = "The Firestore database resource name."
  value       = google_firestore_database.default.name
}

output "pubsub_topic" {
  description = "Full id of the AgentBBS events Pub/Sub topic."
  value       = google_pubsub_topic.events.id
}

output "pubsub_topic_name" {
  description = "Short name of the events topic (used by the reporter)."
  value       = google_pubsub_topic.events.name
}

output "pubsub_subscription" {
  description = "Full id of the events subscription."
  value       = google_pubsub_subscription.events.id
}

output "function_name" {
  description = "Name of the deployed sysop-report Cloud Function."
  value       = google_cloudfunctions2_function.sysop_report.name
}

output "function_uri" {
  description = "The underlying Cloud Run service URI of the function (if any)."
  value       = google_cloudfunctions2_function.sysop_report.service_config[0].uri
}
