variable "zone_id" {
  description = "Cloudflare zone id for the mailbox domain."
  type        = string
}

variable "worker_host" {
  description = "Public Worker hostname, for example mail.example.com."
  type        = string
}

variable "create_mailbox_requests_per_minute" {
  description = "Rate limit for POST /api/mailbox per visitor."
  type        = number
  default     = 10
}

variable "websocket_connect_requests_per_minute" {
  description = "Rate limit for GET /api/mailbox/connect per visitor."
  type        = number
  default     = 30
}

variable "api_requests_per_minute" {
  description = "Catch-all rate limit for /api/* per visitor."
  type        = number
  default     = 300
}

variable "mitigation_timeout_seconds" {
  description = "How long Cloudflare blocks a visitor after a rate limit is hit."
  type        = number
  default     = 60
}
