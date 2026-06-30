# smails-rs

Rust implementation of the smails Cloudflare shape with Workers, Durable
Objects, Workers Assets, a Yew frontend, a CLI, and an MCP stdio server.

## Architecture

```text
Browser
  |
  | https://<site-domain>/
  v
Cloudflare Worker fetch handler
  |-- serves Yew static assets from crates/frontend/static
  |-- handles /api/mailbox, /api/mailbox/messages, /api/mailbox/connect
  |-- routes mailbox state to one Durable Object per mailbox name
  |
  v
Durable Object MAILBOX
  |-- stores mailbox token and parsed messages in DO SQLite
  |-- expires inactive mailboxes with DO alarms
  |-- streams new-message events with hibernatable WebSockets

Cloudflare Email Routing
  |
  | email for <mailbox-domain>
  v
Worker email handler
  |-- rejects oversized raw messages
  |-- parses body and attachment metadata only
  |-- delivers parsed messages to the mailbox Durable Object

CLI / MCP
  |
  | HTTPS API base URL
  v
same Worker /api endpoints
```

The Yew frontend is served by the same Worker as the API, so browser calls use
relative paths such as `/api/mailbox`. It does not need its own API domain
configuration.

## Layout

```text
Cargo.toml                 # workspace
wrangler.toml              # Cloudflare Worker deploy/dev entrypoint
crates/
  core/                    # wasm-safe DTOs, paths, token helpers
  worker/                  # fetch/email/Durable Object entrypoints
  frontend/                # Yew CSR app and static asset shell
  native/                  # native-only config and HTTP API client
  cli/                     # smails CLI binary
  mcp/                     # MCP stdio server
```

## Configuration Model

There are three separate domain concepts:

```text
site/API domain      https://mail.example.com
mailbox domain       example.com
CLI/MCP API base URL https://mail.example.com
```

The site/API domain is Cloudflare routing configuration. It is where the
frontend, REST API, and WebSocket endpoint are served.

The mailbox domain is a Worker runtime variable:

```toml
[vars]
MAILBOX_DOMAINS = "example.com,alt.example.com"
```

The first mailbox domain is used when a create request does not specify a
domain. `MAILBOX_DOMAINS` is not a secret; do not encrypt it in the Cloudflare
dashboard.

The CLI and MCP server are native clients. They default to local Wrangler dev
and should be pointed at production explicitly:

```bash
smails create --api-url https://mail.example.com
```

The selected API base URL is saved in the local config with the mailbox token.
For first-time MCP-only setup, pass `api_url` to the MCP `create_mailbox` tool
or set `SMAILS_API_URL` in the MCP server environment.

## Local Development

```bash
mise run setup
mise run check
mise run build
mise run dev
```

With `wrangler dev` running:

```bash
mise run probe
mise run cli -- create
mise run cli -- inbox
mise run mcp
```

Local HTTP probes do not prove Cloudflare Email Routing. Real email ingress
must be checked after deployment.

## Deploy With GitHub Actions

1. Choose the domains:

```text
site/API domain: https://mail.example.com
mailbox domain:  example.com
```

2. Add GitHub repository variables:

```text
WORKER_DOMAIN=mail.example.com
MAILBOX_DOMAINS=example.com
```

3. Add GitHub repository secrets:

```text
CLOUDFLARE_API_TOKEN=<token allowed to deploy this Worker>
CLOUDFLARE_ACCOUNT_ID=<optional account id if Wrangler cannot infer it>
```

4. Push to `main` or run the `CI` workflow manually.

The workflow sets up Rust through `actions-rust-lang/setup-rust-toolchain@v1`
and uses `jdx/mise-action@v2` for repo tools and tasks.

It runs:

```bash
mise run setup
mise run check
mise run build
```

On `main`, it deploys with:

```text
mise run deploy
```

Cloudflare's online Worker deploy environment may not have `cargo`, so do not
use the dashboard build step for this Rust project unless you install Rust
there first.

## Deploy Locally

Local deploy uses the same explicit domain inputs:

```bash
WORKER_DOMAIN=mail.example.com MAILBOX_DOMAINS=example.com mise run deploy
```

## Configure Email Routing

After the Worker is deployed, configure Cloudflare Email Routing for the
mailbox domain.

Route the desired addresses or catch-all for `example.com` to this Worker. This
is separate from the HTTP custom domain. Without Email Routing, the frontend and
API can work while inbound email still never reaches the `email()` handler.

If configuring variables in the Cloudflare dashboard, use:

```text
Variable name:  MAILBOX_DOMAINS
Variable value: example.com
Encrypt:        off
```

## Verify Production

```bash
curl https://mail.example.com/health
smails create --api-url https://mail.example.com
smails inbox
```

Then send a real email to the generated address and confirm it appears in the
web UI, CLI, or MCP `list_messages` / `read_message`.

Cloudflare references:

- Workers custom domains: https://developers.cloudflare.com/workers/configuration/routing/custom-domains/
- Worker environment variables: https://developers.cloudflare.com/workers/configuration/environment-variables/
- Email handler and routing: https://developers.cloudflare.com/email-service/api/route-emails/email-handler/

## Core API

```text
POST /api/mailbox
GET  /api/domains
GET  /api/mailbox/messages
GET  /api/mailbox/messages/:id
DEL  /api/mailbox/messages/:id
WS   /api/mailbox/connect?token=<token>
GET  /health
```

All mailbox message endpoints require:

```text
Authorization: Bearer <token>
```

## Notes

- Attachments are not stored. Only metadata such as filename, content type,
  content id, disposition, and size is kept with the message.
- Raw email is read into memory only after the size guard passes.
- Native-only code lives in `crates/native`, `crates/cli`, and `crates/mcp`.
- Only `core`, `worker`, and `frontend` are checked for `wasm32-unknown-unknown`
  compatibility.
