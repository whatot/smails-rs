# Operations

## Built-in Protections

The Worker has baseline protections in code:

```text
mailbox create body        4 KB max
incoming email raw size    512 KB max
messages per mailbox       100 newest messages
Worker CPU per request     100 ms max
```

Unknown mailbox email delivery returns before MIME parsing and storage. This
keeps Email Routing abuse from expanding into attachment parsing, message
storage, or per-mailbox Durable Object state.

## Optional WAF Rate Limiting

For production, additionally manage Cloudflare WAF rate limiting rules in the
private `whatot-cf-infra` repository. These rules block traffic before it reaches
the Worker, but the Worker does not depend on them for basic protection.

Required Cloudflare API token permissions:

```text
Zone:
  WAF Write
  Zone Read
```

Current optional WAF rules:

```text
POST /api/mailbox          10 req/min per visitor
GET  /api/mailbox/connect  30 req/min per visitor
/api/*                     300 req/min per visitor
```

Cloudflare Email Routing does not pass through these HTTP WAF rules. Email abuse
is handled by the Worker protections above.

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
total_mailboxes_created  cumulative successful mailbox creations
```

It does not store mailbox names, mailbox tokens, message subjects, senders,
bodies, attachment contents, or per-message counters. Email delivery and
rejected/unknown mailbox traffic do not write to the Admin Durable Object.

For production, Cloudflare Access can additionally protect `/admin/*` before
requests reach the Worker.
