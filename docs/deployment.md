# Deployment

This project should be deployed from GitHub Actions or a local machine with the
Rust toolchain installed. Cloudflare's online Worker deploy environment may not
have `cargo`, so the dashboard build step is not the default path for this Rust
Worker.

## Domain Model

There are three separate values:

```text
WORKER_DOMAIN    mail.example.com
MAILBOX_DOMAINS  example.com,alt.example.com
CLI/MCP API URL  https://mail.example.com
```

`WORKER_DOMAIN` is the HTTP site/API hostname. The Yew frontend and Worker API
are served from the same Worker, so the browser calls relative paths such as
`/api/mailbox`.

`MAILBOX_DOMAINS` is a Worker runtime variable. It controls which email domains
can be used when creating mailboxes. It is not a secret and should not be
encrypted in the Cloudflare dashboard.

The CLI and MCP server are native clients. Point them at production explicitly:

```bash
smails create --api-url https://mail.example.com
```

## Cloudflare API Token

Create a Cloudflare API token and store it as the GitHub secret
`CLOUDFLARE_API_TOKEN`.

The simple path is Cloudflare's `Edit Cloudflare Workers` token template, scoped
to the target account and zone. It includes a few permissions this project does
not currently use, but it matches Wrangler's normal deploy path and avoids a
fragile hand-picked token.

If creating a custom token, use at least:

```text
Account:
  Workers Scripts: Write
  Account Settings: Read

Zone:
  Workers Routes: Write

User:
  User Details: Read
  User Memberships: Read
```

Why these are needed:

```text
Workers Scripts: Write   upload the Worker, assets, source maps, and DO config
Workers Routes: Write    bind WORKER_DOMAIN through wrangler deploy --domain
Account/User Read        let Wrangler resolve the account and membership
```

`CLOUDFLARE_ACCOUNT_ID` is optional if Wrangler can infer the account, but it is
fine to store it as a GitHub secret to remove ambiguity.

Email Routing configuration is separate from `wrangler deploy`. Do not add Email
Routing token permissions unless CI will also manage email routes.

References:

- https://developers.cloudflare.com/workers/ci-cd/external-cicd/github-actions/
- https://developers.cloudflare.com/fundamentals/api/reference/template/

## GitHub Actions Setup

Add repository variables:

```text
WORKER_DOMAIN=mail.example.com
MAILBOX_DOMAINS=example.com
```

For multiple mailbox domains, use one comma-separated value in the repository
variable. Spaces are trimmed, but keeping it compact is easier to scan:

```text
MAILBOX_DOMAINS=example.com,alt.example.com
```

Do not include `https://`, path segments, or the Worker site/API hostname unless
that hostname is also an Email Routing mailbox domain.

Add repository secrets:

```text
CLOUDFLARE_API_TOKEN=<Cloudflare API token>
CLOUDFLARE_ACCOUNT_ID=<optional Cloudflare account id>
```

Push to `main` or run the `CI` workflow manually. The workflow runs:

```bash
mise run setup
mise run check
mise run build
mise run deploy
```

Deploy only runs on `main`, not pull requests.

## Local Deploy

Use the same explicit inputs locally:

```bash
WORKER_DOMAIN=mail.example.com MAILBOX_DOMAINS=example.com mise run deploy
```

The deploy task runs:

```bash
wrangler deploy --keep-vars --var "MAILBOX_DOMAINS:$MAILBOX_DOMAINS" --domain "$WORKER_DOMAIN"
```

`--domain` binds the Worker site/API hostname. If you configure the custom
domain manually in Cloudflare instead, keep `WORKER_DOMAIN` consistent with that
hostname for CLI/MCP usage and production checks.

## Cloudflare Dashboard Values

If configuring the Worker variable in the dashboard, use:

```text
Variable name:  MAILBOX_DOMAINS
Variable value: example.com
Encrypt:        off
```

Do not put `WORKER_DOMAIN` there. It is routing/deploy input, not runtime app
configuration.

## Email Routing

After the Worker is deployed, configure Cloudflare Email Routing for each
mailbox domain in `MAILBOX_DOMAINS`.

This is separate from the HTTP custom domain. Without Email Routing, the
frontend and API can work while inbound email never reaches the Worker's
`email()` handler.

1. In the Cloudflare dashboard, select the zone for `example.com`.
2. Go to `Compute` > `Email Service` > `Email Routing`.
3. Select `Onboard Domain`.
4. Let Cloudflare add the required DNS records, or add the shown records
   yourself:

```text
MX   route inbound mail to Cloudflare
TXT  SPF authorization
TXT  DKIM authentication
```

5. Wait until the domain shows as enabled.
6. Open `Routing Rules`.
7. For temporary mailboxes, enable `Catch-all rule`.
8. Set the catch-all action to `Send to a Worker`.
9. Select this Worker, for example `smails-rs`.
10. Save.

The catch-all rule is the right default for this project because mailbox local
parts are generated dynamically. A normal routing rule for one fixed local part
also works, but only that exact address receives mail.

Keep these values aligned:

```text
Cloudflare Email Routing zone  example.com
MAILBOX_DOMAINS                example.com
Worker selected in rule        smails-rs
```

Reference:

- https://developers.cloudflare.com/email-service/get-started/route-emails/
- https://developers.cloudflare.com/email-service/configuration/email-routing-addresses/

## Verify Production

```bash
curl https://mail.example.com/health
smails create --api-url https://mail.example.com
smails inbox
```

Then send a real email to the generated address and confirm it appears in the
web UI, CLI, or MCP `list_messages` / `read_message`.
