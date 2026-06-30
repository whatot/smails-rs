# Architecture

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

## Notes

- Attachments are not stored. Only metadata such as filename, content type,
  content id, disposition, and size is kept with the message.
- Raw email is read into memory only after the size guard passes.
- Native-only code lives in `crates/native`, `crates/cli`, and `crates/mcp`.
- Only `core`, `worker`, and `frontend` are checked for `wasm32-unknown-unknown`
  compatibility.
