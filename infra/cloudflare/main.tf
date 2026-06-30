locals {
  worker_host = lower(var.worker_host)
}

resource "cloudflare_ruleset" "smails_rate_limits" {
  zone_id     = var.zone_id
  name        = "smails rate limits"
  description = "Rate limits for smails public Worker endpoints."
  kind        = "zone"
  phase       = "http_ratelimit"

  rules = [
    {
      action      = "block"
      description = "Limit mailbox creation"
      enabled     = true
      expression  = "http.host eq \"${local.worker_host}\" and http.request.method eq \"POST\" and http.request.uri.path eq \"/api/mailbox\""
      ratelimit = {
        characteristics     = ["cf.colo.id", "ip.src"]
        period              = 60
        requests_per_period = var.create_mailbox_requests_per_minute
        mitigation_timeout  = var.mitigation_timeout_seconds
      }
    },
    {
      action      = "block"
      description = "Limit mailbox websocket connects"
      enabled     = true
      expression  = "http.host eq \"${local.worker_host}\" and http.request.method eq \"GET\" and http.request.uri.path eq \"/api/mailbox/connect\""
      ratelimit = {
        characteristics     = ["cf.colo.id", "ip.src"]
        period              = 60
        requests_per_period = var.websocket_connect_requests_per_minute
        mitigation_timeout  = var.mitigation_timeout_seconds
      }
    },
    {
      action      = "block"
      description = "Limit all API requests"
      enabled     = true
      expression  = "http.host eq \"${local.worker_host}\" and starts_with(http.request.uri.path, \"/api/\")"
      ratelimit = {
        characteristics     = ["cf.colo.id", "ip.src"]
        period              = 60
        requests_per_period = var.api_requests_per_minute
        mitigation_timeout  = var.mitigation_timeout_seconds
      }
    }
  ]
}
