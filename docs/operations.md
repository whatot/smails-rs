# Operations

## Built-in Protections

The Worker has baseline protections in code:

```text
mailbox create body        4 KB max
incoming email raw size    512 KB max
messages per mailbox       100 newest messages
mailbox lifetime           7 days since last use
attachments                metadata only; content is discarded
Worker CPU per request     Cloudflare plan default
```

Unknown mailbox email delivery returns before MIME parsing and storage. This
keeps Email Routing abuse from expanding into attachment parsing, message
storage, or per-mailbox Durable Object state.

## Optional Edge Rate Limiting

Cloudflare Free tier cannot create the WAF rate limiting rules this project
would use. On Free tier, rely on the built-in Worker protections above and watch
Worker/Durable Object usage.

If the account is upgraded or already has WAF rate limiting, manage the optional
edge rules in the private `whatot-cf-infra` repository. These rules block HTTP
traffic before it reaches the Worker, but the Worker does not depend on them for
basic protection.

Required Cloudflare API token permissions:

```text
Zone:
  WAF Write
  Zone Read
```

Candidate optional rules:

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
