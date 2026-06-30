# Operations

## Rate Limiting

HTTP rate limits are managed with OpenTofu in `infra/cloudflare`.

Required Cloudflare API token permissions:

```text
Zone:
  WAF Write
  Zone Read
```

Set variables with `terraform.tfvars`, `*.auto.tfvars`, or environment
variables:

```bash
export TF_VAR_zone_id=<cloudflare-zone-id>
export TF_VAR_worker_host=mail.example.com
```

Then run:

```bash
mise run cf-init
mise run cf-plan
mise run cf-apply
```

Current rules:

```text
POST /api/mailbox          10 req/min per visitor
GET  /api/mailbox/connect  30 req/min per visitor
/api/*                     300 req/min per visitor
```

Cloudflare Email Routing does not pass through these HTTP WAF rules. Email abuse
is limited in Worker code with raw-size rejection, unknown-mailbox early return,
and per-mailbox message retention.

## Admin API

Set `ADMIN_TOKEN` as a Worker secret:

```bash
wrangler secret put ADMIN_TOKEN
```

Read stats:

```bash
curl https://mail.example.com/admin/stats \
  -H "authorization: Bearer $ADMIN_TOKEN"
```

The admin path only stores low-frequency counters:

```text
total_mailboxes
rejected_large_messages
unknown_mailbox_deliveries
```

It does not store mailbox names, mailbox tokens, message subjects, senders,
bodies, attachment contents, or per-message counters. Normal successful email
delivery does not write to the Admin Durable Object.

For production, Cloudflare Access can additionally protect `/admin/*` before
requests reach the Worker.
