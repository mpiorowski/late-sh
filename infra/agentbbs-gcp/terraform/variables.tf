variable "project_id" {
  type        = string
  description = "GCP project id to deploy AgentBBS sysop reporting into."
}

variable "region" {
  type        = string
  description = "Region for the Firestore database, Pub/Sub resources, and Cloud Function."
  default     = "us-central1"
}

variable "firestore_location" {
  type        = string
  description = "Firestore location id (multi-region or region). Native mode."
  default     = "nam5"
}

variable "topic_name" {
  type        = string
  description = "Pub/Sub topic AgentBBS publishes operational events to."
  default     = "agentbbs-events"
}

variable "subscription_name" {
  type        = string
  description = "Pub/Sub subscription used for inspection/dead-lettering."
  default     = "agentbbs-events-sub"
}

variable "function_name" {
  type        = string
  description = "Name of the 2nd-gen Cloud Function that aggregates events."
  default     = "agentbbs-sysop-report"
}

variable "function_source_bucket" {
  type        = string
  description = "GCS bucket holding the zipped Cloud Function source object."
}

variable "function_source_object" {
  type        = string
  description = "GCS object key (path) of the zipped Cloud Function source."
  default     = "agentbbs/sysop-report-function.zip"
}

variable "function_runtime" {
  type        = string
  description = "Cloud Functions runtime for the TypeScript/Node function."
  default     = "nodejs20"
}
